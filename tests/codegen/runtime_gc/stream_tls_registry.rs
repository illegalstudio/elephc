//! Purpose:
//! TDD regressions for TLS sessions and transport metadata owned by stream registry entries.
//!
//! Called from:
//! - `cargo test --test codegen_tests test_stream_tls_registry_` once this module is wired.
//!
//! Key details:
//! - Local Rustls servers reuse the certificate fixture from the existing HTTPS tests.
//! - A bounded worker pool serves stress fixtures without spawning one thread per stream.
//! - Nonblocking accepts, a deadline, and an explicit stop guard prevent failed clients from hanging tests.
//! - PHP 8.5.6 oracles pin socket capacity and real-TLS enable/disable behavior.
//! - No hermetic AUTH TLS plus PASV fixture exists, so FTPS control/data error cleanup remains uncovered here.

use crate::support::*;

const TLS_PROBE_ACCEPT_DEADLINE: std::time::Duration = std::time::Duration::from_secs(300);
const TLS_PROBE_ACCEPT_POLL: std::time::Duration = std::time::Duration::from_millis(5);
const TLS_PROBE_IO_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);
/// How long a served connection waits for its trigger byte.
///
/// It cannot share the 5-second exchange timeout, because the fixtures that hold many sessions
/// open send nothing until every session exists: connection 1 waits out the client's whole open
/// loop. On a loaded runner opening 257 TLS sessions takes longer than that, the first worker
/// timed out, and its error stopped the server for all the others — which is why CI saw every
/// read fail and a session count that moved between runs. The wait is bounded by the accept
/// deadline instead, which already covers the same loop.
const TLS_PROBE_TRIGGER_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(120);
/// The pool must cover every connection the probe holds open AT ONCE, not just the
/// busiest moment of a sequential exchange: a worker completes its handshake and then
/// blocks reading, while `supports_more_than_256_live_tls_sessions` opens all 257
/// streams before writing to any of them. A pool of 16 therefore stalled the 17th
/// handshake and capped that test at 16 sessions regardless of what the compiler does.
const TLS_PROBE_WORKER_LIMIT: usize = 320;

const TEST_TLS_CERT_PEM: &str = "\
-----BEGIN CERTIFICATE-----
MIIDDTCCAfWgAwIBAgIUYwEnFCptGtZ9bISKGHSDDyDeR78wDQYJKoZIhvcNAQEL
BQAwFjEUMBIGA1UEAwwLZWxlcGhjLXRlc3QwHhcNMjYwNjAxMTQzMzMzWhcNMzYw
NTI5MTQzMzMzWjAWMRQwEgYDVQQDDAtlbGVwaGMtdGVzdDCCASIwDQYJKoZIhvcN
AQEBBQADggEPADCCAQoCggEBALEueBZ5lUAbSBPd5gj6DdreVaIUC1sTKaOtK32f
gEgo8f+OvI7x0xZSB75t07Kz4luusaq1iYKegF61P8gI0ZpaNkj6uLVowj+Pu8/+
AMPrr11i38P701YLNvcOf4QWOnoDlRsjyzR+w4XbQmeNRrT1yUwkUQf64rZ3OkrD
tk4+VLizdj/eeoEXezGO/HzEY4vyFHA0ZC4GDT0yfjh77NOi7rY+7yr1DdbYzon/
JkPw3fV25m7StGsgr/a3i4ghVXUze88XSAYHWANUMmyJc2kxX33EAWB30n5yy0DN
ikN8emJqsRhpVU4MwlnD+5tPVBz9rgdXE8++I5i5uUvX65UCAwEAAaNTMFEwHQYD
VR0OBBYEFKx0E1bLjEIQqIzIzj0qhgpMIg0WMB8GA1UdIwQYMBaAFKx0E1bLjEIQ
qIzIzj0qhgpMIg0WMA8GA1UdEwEB/wQFMAMBAf8wDQYJKoZIhvcNAQELBQADggEB
AKeskQbHp//yz/LEJWqa2uCKB+05Uutg/yauByw2JGvFIdpGMXtOeFYh6PlbhVQL
rijdbW0mI0W2slefK6xsCJxFGfQY3daL2pLgoJSU0nkW7WkZh0ao292letIR9vFR
8cULtOtZZUSl8lq6Xt51mdUcCvAJgNctEI/+58YyDZBrUf0hKSjAQ2MGuZsHr8xT
S5TYFmrdKicmU53hVXsNgsCDmqENsZqP99zgqikvcrd1qfJQ95N/7thuSJtBJydk
IxMlsDmy7cFWp8ts9w+WvdxpGeZAs1M7I2N2SqTuHYVh3SJCrdA1rwtJZKTsctUJ
rmggbINQyJdm1RdcppwbOqA=
-----END CERTIFICATE-----
";

const TEST_TLS_KEY_PEM: &str = "\
-----BEGIN PRIVATE KEY-----
MIIEvQIBADANBgkqhkiG9w0BAQEFAASCBKcwggSjAgEAAoIBAQCxLngWeZVAG0gT
3eYI+g3a3lWiFAtbEymjrSt9n4BIKPH/jryO8dMWUge+bdOys+JbrrGqtYmCnoBe
tT/ICNGaWjZI+ri1aMI/j7vP/gDD669dYt/D+9NWCzb3Dn+EFjp6A5UbI8s0fsOF
20JnjUa09clMJFEH+uK2dzpKw7ZOPlS4s3Y/3nqBF3sxjvx8xGOL8hRwNGQuBg09
Mn44e+zTou62Pu8q9Q3W2M6J/yZD8N31duZu0rRrIK/2t4uIIVV1M3vPF0gGB1gD
VDJsiXNpMV99xAFgd9J+cstAzYpDfHpiarEYaVVODMJZw/ubT1Qc/a4HVxPPviOY
ublL1+uVAgMBAAECggEAKW0fAMo+njWCvbplHXYxpRnU1cdv/ERXuQA1KfMQEE8a
fdEGvzlFTHOzgc+17pNmel83BR3a3+JlSz9/gSqmrzsmdBvC8g9jU28sz22pCiXh
46jJfs4zVGvc1xjZsa1s0LhjtWvCCC0XVAW22fVLMeZBwX7AP2hmd5ka1P47csF2
aDIPRPuWWCMse7u/31bJIpLOTJwLe1KmOsrk8IaQcjPUYC+WCA84N3QUwVUMVXvR
31bYy2s2fLZ/pO4EYCHJ2TDXuUSL4JYQ9ru7FPNWyGQo8cuTBexDWMiRb8qxFYNl
U5pAJuk4Om2v3CqIgCLK2PQB/lPrJkcUPEN4P5SGgQKBgQDeZux9GFcYpwZKTAr2
4rPU7ovCNTgAGyNh+5u/xaJ/6zNYDKH+EQujM35JhZR114nHYvigTzUj2VyTPMEq
ncyYoG+7sj99QqMNqIXK+d22UeYWmbSw/jf1XDzC7UHWXASViw/kL1y/jP4NXSjf
dAxSahyRnP+aYYNXAsmRWsV2YQKBgQDL8rUFs1nzX6WfHRQ5zzcPAF9XAGwkVKzQ
OKHCHfyLN9sfCnJrSOd1DU3JEwWZ6Qzl+BwAavaqDHY8PsV0pMtKSfO77yDZVFeE
ZdrJeQMv44DszZjZK/J9Vd7JDR+6Yg49+P4l438KrMsbIp/PaEe34ApgwfzU1LB5
XOORMcPZtQKBgQCk7CAc1+rmbh19BQzwbca7dTYQi1R+x6EibOnfeRh60Zieh6es
90jw+iOBM9yW0oHqaJtEjdgzQGGlEd2Q07m/yOFyh8kLA1pUq46jqUzfgbYlNlBH
HA21FnQ8fKJg6pW/q4LaTMDzjwNqN5YytiTZDLUoygrFmeBCqt98uZpKoQKBgB7W
5pSkGDf7AJpc1VAgi1zTW5dWUwPzYeZiieNGkYejvJinBcI/VfCXQGnlXHV3jiHA
MMvHYOE53S8i9sy6lpr3L8n9UORMIqe8lybcC6VUK4yjUjeUs6hMMdIJEAEpDqpE
Wnn0OqOsmVHTHINKa33cfPVAoDC2sLDJYQf1lH35AoGAd0pIqclrFb1a4Fbpq8TM
jgOspoq2Sjj+5724t8sFeg7SRMdTkA/8M1t4FsY9TNhDSI2vi6cu9013EcfVGlUB
MYQgldWOaXCRMQsHgapn+orK7iF89zA+4UDACVNiHEYS9q8CGynLckruklWdiyi3
6NdfPEjH08mFJU5npyEEa7Q=
-----END PRIVATE KEY-----
";

#[derive(Clone, Copy)]
enum TlsReplyMode {
    FixedOk,
    ConnectionIndex,
    ServerName,
}

struct TlsProbeServer {
    stop: std::sync::Arc<std::sync::atomic::AtomicBool>,
    thread: Option<std::thread::JoinHandle<Result<(), String>>>,
}

impl TlsProbeServer {
    /// Waits for every expected connection and reports listener or worker failures.
    fn join(mut self) -> Result<(), String> {
        let thread = self
            .thread
            .take()
            .expect("TLS registry test: server thread present");
        thread
            .join()
            .map_err(|_| "TLS registry test: server thread panicked".to_string())?
    }
}

impl Drop for TlsProbeServer {
    /// Stops nonblocking acceptance and joins workers when a test unwinds early.
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

/// Builds the local Rustls server configuration shared by TLS registry tests.
fn tls_server_config() -> std::sync::Arc<rustls::ServerConfig> {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let mut cert_reader = TEST_TLS_CERT_PEM.as_bytes();
    let certs = rustls_pemfile::certs(&mut cert_reader)
        .collect::<Result<Vec<_>, _>>()
        .expect("TLS registry test: parse certificate");
    let mut key_reader = TEST_TLS_KEY_PEM.as_bytes();
    let key = rustls_pemfile::private_key(&mut key_reader)
        .expect("TLS registry test: parse private key")
        .expect("TLS registry test: private key present");
    std::sync::Arc::new(
        rustls::ServerConfig::builder()
            .with_no_client_auth()
            .with_single_cert(certs, key)
            .expect("TLS registry test: build server configuration"),
    )
}

/// Completes one TLS exchange and closes promptly so a bounded worker can serve another stream.
fn serve_tls_probe_connection(
    tcp: std::net::TcpStream,
    config: std::sync::Arc<rustls::ServerConfig>,
    reply_mode: TlsReplyMode,
    index: usize,
) -> Result<(), String> {
    use std::io::{Read, Write};

    // The listener is nonblocking so the accept loop can honour its deadline. On
    // BSD/macOS the socket returned by accept() inherits O_NONBLOCK, which
    // set_read_timeout does not clear, so the handshake read below fails with
    // WouldBlock before a single byte arrives. Linux does not inherit the flag,
    // which is why this only ever showed up here.
    tcp.set_nonblocking(false)
        .map_err(|error| format!("connection {index}: clear nonblocking: {error}"))?;
    tcp.set_read_timeout(Some(TLS_PROBE_TRIGGER_TIMEOUT))
        .map_err(|error| format!("connection {index}: set read timeout: {error}"))?;
    tcp.set_write_timeout(Some(TLS_PROBE_IO_TIMEOUT))
        .map_err(|error| format!("connection {index}: set write timeout: {error}"))?;
    let conn = rustls::ServerConnection::new(config)
        .map_err(|error| format!("connection {index}: create TLS server: {error}"))?;
    let mut tls = rustls::StreamOwned::new(conn, tcp);
    let mut trigger = [0u8; 1];
    tls.read_exact(&mut trigger)
        .map_err(|error| format!("connection {index}: read trigger: {error}"))?;
    // The trigger has arrived, so the rest of the exchange is one round trip and goes back to
    // the tight budget.
    tls.get_ref()
        .set_read_timeout(Some(TLS_PROBE_IO_TIMEOUT))
        .map_err(|error| format!("connection {index}: restore read timeout: {error}"))?;
    let reply = match reply_mode {
        TlsReplyMode::FixedOk => b"ok".to_vec(),
        TlsReplyMode::ConnectionIndex => {
            vec![b'A' + u8::try_from(index).expect("small connection index")]
        }
        TlsReplyMode::ServerName => tls
            .conn
            .server_name()
            .unwrap_or("")
            .as_bytes()
            .to_vec(),
    };
    tls.write_all(&reply)
        .map_err(|error| format!("connection {index}: write reply: {error}"))?;
    tls.flush()
        .map_err(|error| format!("connection {index}: flush reply: {error}"))
}

/// Spawns a loopback TLS server that replies once on each accepted connection.
fn spawn_tls_probe_server(
    connection_count: usize,
    reply_mode: TlsReplyMode,
) -> (TlsProbeServer, u16) {
    let listener =
        std::net::TcpListener::bind(("127.0.0.1", 0)).expect("TLS registry test: bind");
    let port = listener
        .local_addr()
        .expect("TLS registry test: local address")
        .port();
    listener
        .set_nonblocking(true)
        .expect("TLS registry test: nonblocking listener");
    let config = tls_server_config();
    let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let server_stop = std::sync::Arc::clone(&stop);
    let thread = std::thread::spawn(move || -> Result<(), String> {
        let (jobs, receiver) =
            std::sync::mpsc::channel::<(usize, std::net::TcpStream)>();
        let receiver = std::sync::Arc::new(std::sync::Mutex::new(receiver));
        let worker_count = connection_count.min(TLS_PROBE_WORKER_LIMIT).max(1);
        let mut workers = Vec::with_capacity(worker_count);
        for _ in 0..worker_count {
            let receiver = std::sync::Arc::clone(&receiver);
            let config = std::sync::Arc::clone(&config);
            let worker_stop = std::sync::Arc::clone(&server_stop);
            workers.push(std::thread::spawn(move || -> Result<(), String> {
                loop {
                    if worker_stop.load(Ordering::SeqCst) {
                        return Ok(());
                    }
                    let job = receiver
                        .lock()
                        .map_err(|_| "TLS probe job queue lock poisoned".to_string())?
                        .recv();
                    let Ok((index, tcp)) = job else {
                        return Ok(());
                    };
                    if let Err(error) = serve_tls_probe_connection(
                        tcp,
                        std::sync::Arc::clone(&config),
                        reply_mode,
                        index,
                    )
                    {
                        worker_stop.store(true, Ordering::SeqCst);
                        return Err(error);
                    }
                }
            }));
        }

        let deadline = std::time::Instant::now() + TLS_PROBE_ACCEPT_DEADLINE;
        let mut accepted = 0usize;
        let mut accept_error = None;
        while accepted < connection_count && !server_stop.load(Ordering::SeqCst) {
            match listener.accept() {
                Ok((tcp, _)) => {
                    if jobs.send((accepted, tcp)).is_err() {
                        accept_error = Some("TLS probe worker queue closed early".to_string());
                        break;
                    }
                    accepted += 1;
                }
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    if std::time::Instant::now() >= deadline {
                        accept_error = Some(format!(
                            "TLS probe accept deadline reached after {accepted}/{connection_count} connections"
                        ));
                        break;
                    }
                    std::thread::sleep(TLS_PROBE_ACCEPT_POLL);
                }
                Err(error) => {
                    accept_error = Some(format!("TLS probe accept failed: {error}"));
                    break;
                }
            }
        }
        drop(jobs);

        let mut worker_error = None;
        for worker in workers {
            match worker.join() {
                Ok(Ok(())) => {}
                Ok(Err(error)) => {
                    worker_error.get_or_insert(error);
                }
                Err(_) => {
                    worker_error
                        .get_or_insert_with(|| "TLS probe worker panicked".to_string());
                }
            };
        }
        if let Some(error) = worker_error {
            return Err(error);
        }
        if let Some(error) = accept_error {
            return Err(error);
        }
        if !server_stop.load(Ordering::SeqCst) && accepted != connection_count {
            return Err(format!(
                "TLS probe stopped after {accepted}/{connection_count} connections"
            ));
        }
        Ok(())
    });
    (
        TlsProbeServer {
            stop,
            thread: Some(thread),
        },
        port,
    )
}

/// Verifies closing one TLS stream cannot leak its session into a reused descriptor.
#[test]
fn test_stream_tls_registry_fd_reuse_starts_with_fresh_session() {
    let (server, port) = spawn_tls_probe_server(2, TlsReplyMode::ConnectionIndex);
    let out = compile_and_run(
        &r#"<?php
$context = stream_context_get_default();
stream_context_set_option($context, "ssl", "verify_peer", false);
stream_context_set_option($context, "ssl", "verify_peer_name", false);

$first = stream_socket_client("tcp://127.0.0.1:TLS_TEST_PORT");
stream_socket_enable_crypto($first, true, STREAM_CRYPTO_METHOD_TLS_CLIENT);
fwrite($first, "x");
echo fread($first, 1), "|";
fclose($first);

$second = stream_socket_client("tcp://127.0.0.1:TLS_TEST_PORT");
stream_socket_enable_crypto($second, true, STREAM_CRYPTO_METHOD_TLS_CLIENT);
fwrite($second, "y");
echo fread($second, 1), "\n";
fclose($second);
"#
        .replace("TLS_TEST_PORT", &port.to_string()),
    );
    assert_eq!(out, "A|B\n");
    server.join().expect("TLS registry test: server");
}

/// Verifies PHP's real-TLS enable, disable, and re-enable return values.
#[test]
fn test_stream_tls_registry_enable_disable_reenable_matches_php() {
    let (server, port) = spawn_tls_probe_server(1, TlsReplyMode::FixedOk);
    let out = compile_and_run(
        &r#"<?php
$context = stream_context_get_default();
stream_context_set_option($context, "ssl", "verify_peer", false);
stream_context_set_option($context, "ssl", "verify_peer_name", false);
$stream = stream_socket_client("tcp://127.0.0.1:TLS_TEST_PORT");
$enabled = stream_socket_enable_crypto($stream, true, STREAM_CRYPTO_METHOD_TLS_CLIENT);
fwrite($stream, "x");
$reply = fread($stream, 2);
$disabled = @stream_socket_enable_crypto($stream, false);
$reenabled = @stream_socket_enable_crypto($stream, true, STREAM_CRYPTO_METHOD_TLS_CLIENT);
var_dump($enabled);
echo $reply, "\n";
var_dump($disabled);
var_dump($reenabled);
fclose($stream);
"#
        .replace("TLS_TEST_PORT", &port.to_string()),
    );
    assert_eq!(out, "bool(true)\nok\nbool(false)\nbool(false)\n");
    server.join().expect("TLS registry test: server");
}

/// Verifies two simultaneously live TLS streams retain isolated sessions.
#[test]
fn test_stream_tls_registry_two_simultaneous_sessions_are_isolated() {
    let (server, port) = spawn_tls_probe_server(2, TlsReplyMode::ConnectionIndex);
    let out = compile_and_run(
        &r#"<?php
$context = stream_context_get_default();
stream_context_set_option($context, "ssl", "verify_peer", false);
stream_context_set_option($context, "ssl", "verify_peer_name", false);
$first = stream_socket_client("tcp://127.0.0.1:TLS_TEST_PORT");
$firstEnabled = stream_socket_enable_crypto($first, true, STREAM_CRYPTO_METHOD_TLS_CLIENT);
$second = stream_socket_client("tcp://127.0.0.1:TLS_TEST_PORT");
$secondEnabled = stream_socket_enable_crypto($second, true, STREAM_CRYPTO_METHOD_TLS_CLIENT);
fwrite($first, "x");
fwrite($second, "y");
echo ($firstEnabled && $secondEnabled) ? "enabled|" : "failed|";
echo fread($first, 1), "|", fread($second, 1), "\n";
fclose($first);
fclose($second);
"#
        .replace("TLS_TEST_PORT", &port.to_string()),
    );
    assert_eq!(out, "enabled|A|B\n");
    server.join().expect("TLS registry test: server");
}

/// Verifies the SNI host is owned by stream state after its source string is released.
#[test]
fn test_stream_tls_registry_owns_sni_after_source_release() {
    let (server, port) = spawn_tls_probe_server(1, TlsReplyMode::ServerName);
    let out = compile_and_run(
        &r#"<?php
$peerName = $argc > 0 ? "owned.test" : "unused.test";
$context = stream_context_get_default();
stream_context_set_option($context, "ssl", "peer_name", $peerName);
stream_context_set_option($context, "ssl", "verify_peer", false);
stream_context_set_option($context, "ssl", "verify_peer_name", false);
$stream = stream_socket_client("tcp://127.0.0.1:TLS_TEST_PORT");
$peerName = str_repeat("overwritten", 1024);
unset($peerName);
$enabled = stream_socket_enable_crypto($stream, true, STREAM_CRYPTO_METHOD_TLS_CLIENT);
fwrite($stream, "x");
echo $enabled ? "enabled|" : "failed|";
echo fread($stream, 10), "\n";
fclose($stream);
"#
        .replace("TLS_TEST_PORT", &port.to_string()),
    );
    assert_eq!(out, "enabled|owned.test\n");
    server.join().expect("TLS registry test: server");
}

/// Verifies closing a TLS stream invalidates aliases stored in COW containers.
#[test]
fn test_stream_tls_registry_close_invalidates_container_alias() {
    let (server, port) = spawn_tls_probe_server(1, TlsReplyMode::FixedOk);
    let out = compile_and_run(
        &r#"<?php
$context = stream_context_get_default();
stream_context_set_option($context, "ssl", "verify_peer", false);
stream_context_set_option($context, "ssl", "verify_peer_name", false);
$stream = stream_socket_client("tcp://127.0.0.1:TLS_TEST_PORT");
stream_socket_enable_crypto($stream, true, STREAM_CRYPTO_METHOD_TLS_CLIENT);
$aliases = [$stream];
fwrite($stream, "x");
echo fread($stream, 2), "|";
var_dump(fclose($stream));
var_dump(is_resource($aliases[0]));
try {
    fread($aliases[0], 1);
} catch (TypeError $error) {
    echo get_class($error), "\n";
}
"#
        .replace("TLS_TEST_PORT", &port.to_string()),
    );
    assert_eq!(out, "ok|bool(true)\nbool(false)\nTypeError\n");
    server.join().expect("TLS registry test: server");
}

/// Verifies more than 256 simultaneously live TLS sessions remain independently usable.
#[test]
fn test_stream_tls_registry_supports_more_than_256_live_tls_sessions() {
    let (server, port) = spawn_tls_probe_server(257, TlsReplyMode::FixedOk);
    let out = compile_and_run_with_heap_size(
        &r#"<?php
$context = stream_context_get_default();
stream_context_set_option($context, "ssl", "verify_peer", false);
stream_context_set_option($context, "ssl", "verify_peer_name", false);
$streams = [];
for ($i = 0; $i < 257; $i++) {
    $stream = stream_socket_client("tcp://127.0.0.1:TLS_TEST_PORT");
    if ($stream === false) {
        break;
    }
    if (!stream_socket_enable_crypto($stream, true, STREAM_CRYPTO_METHOD_TLS_CLIENT)) {
        fclose($stream);
        break;
    }
    $streams[] = $stream;
}

$errors = 0;
foreach ($streams as $stream) {
    fwrite($stream, "x");
    if (fread($stream, 2) !== "ok") {
        $errors++;
    }
}
echo count($streams), " ", $errors, "\n";
foreach ($streams as $stream) {
    fclose($stream);
}
"#
        .replace("TLS_TEST_PORT", &port.to_string()),
        256 * 1024 * 1024,
    );
    assert_eq!(out, "257 0\n");
    server.join().expect("TLS registry test: server");
}

/// Verifies more than 256 socket resources are not constrained by a raw-fd table.
#[test]
fn test_stream_tls_registry_supports_more_than_256_live_socket_streams() {
    let out = compile_and_run(
        r#"<?php
$pairs = [];
for ($i = 0; $i < 130; $i++) {
    $pair = stream_socket_pair(STREAM_PF_UNIX, STREAM_SOCK_STREAM, STREAM_IPPROTO_IP);
    if ($pair === false) {
        break;
    }
    $pairs[] = $pair;
}
foreach ($pairs as $pair) {
    fwrite($pair[0], "x");
}
$errors = 0;
foreach ($pairs as $pair) {
    if (fread($pair[1], 1) !== "x") {
        $errors++;
    }
}
$closed = 0;
foreach ($pairs as $pair) {
    fclose($pair[0]);
    fclose($pair[1]);
    $closed += 2;
}
echo count($pairs) * 2, " ", $errors, " ", $closed, "\n";
"#,
    );
    assert_eq!(out, "260 0 260\n");
}

/// Verifies generated TLS code no longer publishes or indexes a fixed raw-fd session table.
#[test]
fn test_stream_tls_registry_codegen_has_no_raw_fd_tls_side_table() {
    let id = TEST_ID.fetch_add(1, Ordering::SeqCst);
    let tid = std::thread::current().id();
    let pid = std::process::id();
    let dir = std::env::temp_dir().join(format!("elephc_tls_registry_{pid}_{tid:?}_{id}"));
    fs::create_dir_all(&dir).expect("TLS registry test: create assembly directory");
    let (user_asm, runtime_asm, _) = compile_source_to_asm_with_options(
        r#"<?php
$stream = fopen("php://memory", "r+");
stream_socket_enable_crypto($stream, true, STREAM_CRYPTO_METHOD_TLS_CLIENT);
fclose($stream);
"#,
        &dir,
        8_388_608,
        false,
        false,
    );
    assert!(
        !user_asm.contains("_tls_sessions"),
        "user assembly still indexes the fixed TLS session table"
    );
    assert!(
        !runtime_asm.contains("_tls_sessions"),
        "runtime assembly still publishes the fixed TLS session table"
    );
    let _ = fs::remove_dir_all(dir);
}

/// Pins that every target reads `ssl.verify_peer` before attaching TLS.
///
/// The option selects the non-verifying attach, which is what lets a fixture talk to its own
/// self-signed server. It was read on AArch64 and ignored on x86_64, so all six TLS fixtures
/// failed there and only there — a self-signed peer the host accepted was rejected on Linux
/// x86_64. Asserting on the emitted assembly is what makes the guard reachable from an AArch64
/// host, where the executing tests pass either way.
#[test]
fn test_enable_crypto_reads_verify_peer_on_every_target() {
    for target in ["linux-x86_64", "linux-aarch64", "macos-aarch64"] {
        let dir = make_cli_test_dir("elephc_tls_verify_peer_target");
        let php_path = dir.join("main.php");
        fs::write(
            &php_path,
            r#"<?php
$ctx = stream_context_get_default();
stream_context_set_option($ctx, "ssl", "verify_peer", false);
$s = stream_socket_client("tcp://127.0.0.1:9");
if ($s !== false) {
    stream_socket_enable_crypto($s, true, STREAM_CRYPTO_METHOD_TLS_CLIENT);
}
echo "done";
"#,
        )
        .unwrap();
        let output = elephc_cli_command(&dir)
            .arg("--target")
            .arg(target)
            .arg("--emit-asm")
            .arg(&php_path)
            .output()
            .expect("failed to emit assembly for the TLS attach target");
        assert!(
            output.status.success(),
            "{target}: --emit-asm failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let asm = fs::read_to_string(dir.join("main.s")).expect("target assembly");
        assert!(
            asm.contains("_ssl_verify_peer_key_str"),
            "{target}: the TLS attach must read ssl.verify_peer"
        );
        assert!(
            asm.contains("_elephc_tls_attach_fd_insecure_fn"),
            "{target}: the TLS attach must be able to select the non-verifying variant"
        );
        let _ = fs::remove_dir_all(&dir);
    }
}
