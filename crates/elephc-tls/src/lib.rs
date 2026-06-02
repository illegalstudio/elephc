//! Purpose:
//! Phase 4b TLS surface for the elephc `https://` wrapper.
//!
//! Called from:
//! - Eventually: the elephc runtime, via the `__rt_https_open` helper that
//!   the upcoming `https://` `fopen` lowering will emit.
//! - Today: callable directly from Rust through the `rlib` crate type, and
//!   from C through the `staticlib` (the `extern "C"` entries below).
//!
//! Key details:
//! - One global handle table indexes live TLS sessions by `i64` IDs so the
//!   C ABI can refer to a session without exposing Rust types. Concurrent
//!   callers serialise on the table mutex; that is the v1 trade-off for
//!   simplicity over a per-session lock.
//! - The crypto provider (`ring`) installs itself lazily on the first
//!   `elephc_tls_connect` so callers do not have to thread an init step.
//! - Trust anchors come from `webpki-roots` (Mozilla's bundled CA list);
//!   no system trust-store dependency.
//! - All errors collapse to a sentinel return: `-1` from `connect`/`read`/
//!   `write`. The handle is dropped on `close`; ID reuse is avoided by an
//!   atomic counter.

use std::collections::HashMap;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::os::fd::FromRawFd;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::pki_types::{CertificateDer, PrivateKeyDer, ServerName, UnixTime};
use rustls::{
    ClientConfig, ClientConnection, DigitallySignedStruct, Error as RustlsError, RootCertStore,
    SignatureScheme, Stream,
};

/// Returns the elephc-tls ABI version. v1 = the rustls-backed surface
/// described in this module. The version will be bumped when the C ABI
/// changes shape.
#[no_mangle]
pub extern "C" fn elephc_tls_version() -> i32 {
    1
}

struct HandleEntry {
    sock: TcpStream,
    conn: ClientConnection,
}

fn handles() -> &'static Mutex<HashMap<i64, Box<HandleEntry>>> {
    static HANDLES: OnceLock<Mutex<HashMap<i64, Box<HandleEntry>>>> = OnceLock::new();
    HANDLES.get_or_init(|| Mutex::new(HashMap::new()))
}

fn next_handle_id() -> i64 {
    static NEXT_ID: AtomicI64 = AtomicI64::new(1);
    NEXT_ID.fetch_add(1, Ordering::SeqCst)
}

/// `ServerCertVerifier` that accepts any certificate. Used by
/// `elephc_tls_connect_insecure` when the caller has set the
/// `ssl.verify_peer = false` stream context option. v1 trade-off: TLS still
/// encrypts the channel, but the peer identity is no longer authenticated.
#[derive(Debug)]
struct NoVerification;

impl ServerCertVerifier for NoVerification {
    fn verify_server_cert(
        &self,
        _end_entity: &CertificateDer<'_>,
        _intermediates: &[CertificateDer<'_>],
        _server_name: &ServerName<'_>,
        _ocsp: &[u8],
        _now: UnixTime,
    ) -> Result<ServerCertVerified, RustlsError> {
        Ok(ServerCertVerified::assertion())
    }

    fn verify_tls12_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, RustlsError> {
        Ok(HandshakeSignatureValid::assertion())
    }

    fn verify_tls13_signature(
        &self,
        _message: &[u8],
        _cert: &CertificateDer<'_>,
        _dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, RustlsError> {
        Ok(HandshakeSignatureValid::assertion())
    }

    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        vec![
            SignatureScheme::RSA_PKCS1_SHA256,
            SignatureScheme::RSA_PKCS1_SHA384,
            SignatureScheme::RSA_PKCS1_SHA512,
            SignatureScheme::ECDSA_NISTP256_SHA256,
            SignatureScheme::ECDSA_NISTP384_SHA384,
            SignatureScheme::RSA_PSS_SHA256,
            SignatureScheme::RSA_PSS_SHA384,
            SignatureScheme::RSA_PSS_SHA512,
            SignatureScheme::ED25519,
        ]
    }
}

fn insecure_client_config() -> Arc<ClientConfig> {
    static CFG: OnceLock<Arc<ClientConfig>> = OnceLock::new();
    CFG.get_or_init(|| {
        let _ = rustls::crypto::ring::default_provider().install_default();
        Arc::new(
            ClientConfig::builder()
                .dangerous()
                .with_custom_certificate_verifier(Arc::new(NoVerification))
                .with_no_client_auth(),
        )
    })
    .clone()
}

fn shared_client_config() -> Arc<ClientConfig> {
    static CFG: OnceLock<Arc<ClientConfig>> = OnceLock::new();
    CFG.get_or_init(|| {
        // Install the `ring` crypto provider once per process. Ignore the
        // "already installed" error so multiple connect calls are safe.
        let _ = rustls::crypto::ring::default_provider().install_default();

        let mut roots = RootCertStore::empty();
        roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
        Arc::new(
            ClientConfig::builder()
                .with_root_certificates(roots)
                .with_no_client_auth(),
        )
    })
    .clone()
}

/// Open a TLS-secured TCP connection to `host:port` and return an integer
/// handle ID, or `-1` on any failure. The handle is consumed by
/// `elephc_tls_close`.
///
/// # Safety
///
/// `host_ptr` must point to `host_len` valid UTF-8 bytes for the duration of
/// this call.
#[no_mangle]
pub unsafe extern "C" fn elephc_tls_connect(
    host_ptr: *const u8,
    host_len: usize,
    port: u16,
) -> i64 {
    tls_connect_inner(host_ptr, host_len, port, shared_client_config())
}

/// Variant of `elephc_tls_connect` that uses the `dangerous_configuration`
/// rustls path with a no-op certificate verifier. Surfaced through the
/// runtime when the caller has set `ssl.verify_peer = false` on the stream
/// context. The channel is still encrypted; only the peer identity is
/// unauthenticated.
///
/// # Safety
///
/// Same contract as `elephc_tls_connect`.
#[no_mangle]
pub unsafe extern "C" fn elephc_tls_connect_insecure(
    host_ptr: *const u8,
    host_len: usize,
    port: u16,
) -> i64 {
    tls_connect_inner(host_ptr, host_len, port, insecure_client_config())
}

/// Builds a `ClientConfig` whose trust anchors come from the PEM bundle at
/// `cafile_path` instead of the built-in webpki-roots. Returns `None` if the
/// path is unreadable, contains no certificates, or any certificate is
/// malformed — the caller then fails the connect, matching PHP's behavior when
/// `ssl.cafile` cannot be loaded. Not cached: cafile connects are rare.
fn cafile_client_config(cafile_path: &str) -> Option<Arc<ClientConfig>> {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let pem = std::fs::read(cafile_path).ok()?;
    let mut roots = RootCertStore::empty();
    let mut reader: &[u8] = &pem;
    for cert in rustls_pemfile::certs(&mut reader) {
        roots.add(cert.ok()?).ok()?;
    }
    if roots.is_empty() {
        return None;
    }
    Some(Arc::new(
        ClientConfig::builder()
            .with_root_certificates(roots)
            .with_no_client_auth(),
    ))
}

/// Variant of `elephc_tls_connect` that authenticates the peer against a custom
/// CA bundle (the `ssl.cafile` stream-context option) rather than the built-in
/// webpki-roots trust store. Returns an integer handle ID, or `-1` on any
/// failure (including an unreadable/empty cafile). The secure and insecure
/// connect variants ignore the trailing `cafile_*` arguments, so the runtime
/// can share one call site and just select the function pointer.
///
/// # Safety
///
/// `host_ptr`/`cafile_ptr` must point to `host_len`/`cafile_len` valid UTF-8
/// bytes for the duration of this call.
#[no_mangle]
pub unsafe extern "C" fn elephc_tls_connect_cafile(
    host_ptr: *const u8,
    host_len: usize,
    port: u16,
    cafile_ptr: *const u8,
    cafile_len: usize,
) -> i64 {
    if cafile_ptr.is_null() || cafile_len == 0 {
        return -1;
    }
    let cafile_bytes = std::slice::from_raw_parts(cafile_ptr, cafile_len);
    let cafile_path = match std::str::from_utf8(cafile_bytes) {
        Ok(s) => s,
        Err(_) => return -1,
    };
    let config = match cafile_client_config(cafile_path) {
        Some(c) => c,
        None => return -1,
    };
    tls_connect_inner(host_ptr, host_len, port, config)
}

/// Builds a `ClientConfig` whose trust anchors come from every PEM file in the
/// directory `capath` (the `ssl.capath` stream-context option), instead of the
/// built-in webpki-roots. Each directory entry is read and parsed for X.509
/// certificates; unreadable entries and non-certificate files are skipped.
/// Returns `None` when the directory is unreadable or yields no certificates,
/// so the caller fails the connect — matching PHP's behavior when `ssl.capath`
/// cannot supply a trust store. Not cached: capath connects are rare.
fn capath_client_config(capath: &str) -> Option<Arc<ClientConfig>> {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let mut roots = RootCertStore::empty();
    for entry in std::fs::read_dir(capath).ok()? {
        let path = match entry {
            Ok(e) => e.path(),
            Err(_) => continue,
        };
        if !path.is_file() {
            continue;
        }
        let pem = match std::fs::read(&path) {
            Ok(b) => b,
            Err(_) => continue,
        };
        let mut reader: &[u8] = &pem;
        // Add every certificate the file yields; ignore malformed/non-cert
        // entries so a stray non-PEM file in the directory is not fatal.
        for cert in rustls_pemfile::certs(&mut reader).flatten() {
            let _ = roots.add(cert);
        }
    }
    if roots.is_empty() {
        return None;
    }
    Some(Arc::new(
        ClientConfig::builder()
            .with_root_certificates(roots)
            .with_no_client_auth(),
    ))
}

/// Variant of `elephc_tls_connect` that authenticates the peer against the CA
/// certificates in the directory named by the `ssl.capath` stream-context
/// option. Returns an integer handle ID, or `-1` on any failure (including an
/// unreadable/empty directory). Mirrors `elephc_tls_connect_cafile`; the
/// secure/insecure variants ignore the trailing `capath_*` args so the runtime
/// shares one call site and just selects the function pointer.
///
/// # Safety
///
/// `host_ptr`/`capath_ptr` must point to `host_len`/`capath_len` valid UTF-8
/// bytes for the duration of this call.
#[no_mangle]
pub unsafe extern "C" fn elephc_tls_connect_capath(
    host_ptr: *const u8,
    host_len: usize,
    port: u16,
    capath_ptr: *const u8,
    capath_len: usize,
) -> i64 {
    if capath_ptr.is_null() || capath_len == 0 {
        return -1;
    }
    let capath_bytes = std::slice::from_raw_parts(capath_ptr, capath_len);
    let capath = match std::str::from_utf8(capath_bytes) {
        Ok(s) => s,
        Err(_) => return -1,
    };
    let config = match capath_client_config(capath) {
        Some(c) => c,
        None => return -1,
    };
    tls_connect_inner(host_ptr, host_len, port, config)
}

/// Variant of `elephc_tls_connect` that authenticates the peer against the
/// built-in webpki-roots but verifies the certificate for the host named by
/// the `ssl.peer_name` stream-context option instead of the connection host.
/// Used when a program connects to one address (e.g. an IP or alternate name)
/// but the certificate is issued for a different hostname. `peer_name` is also
/// sent as the SNI server name. Returns a handle ID, or `-1` on failure.
///
/// # Safety
///
/// `host_ptr`/`peer_ptr` must point to `host_len`/`peer_len` valid UTF-8 bytes
/// for the duration of this call.
#[no_mangle]
pub unsafe extern "C" fn elephc_tls_connect_peer_name(
    host_ptr: *const u8,
    host_len: usize,
    port: u16,
    peer_ptr: *const u8,
    peer_len: usize,
) -> i64 {
    if peer_ptr.is_null() || peer_len == 0 {
        return -1;
    }
    let peer_bytes = std::slice::from_raw_parts(peer_ptr, peer_len);
    let peer_name = match std::str::from_utf8(peer_bytes) {
        Ok(s) => s,
        Err(_) => return -1,
    };
    tls_connect_inner_named(host_ptr, host_len, port, shared_client_config(), Some(peer_name))
}

/// Builds a `ClientConfig` that presents a client certificate for mutual TLS:
/// the PEM certificate chain at `cert_path` and the PEM private key at
/// `key_path` (the `ssl.local_cert` / `ssl.local_pk` stream-context options).
/// The server is still authenticated against the built-in webpki-roots.
/// Returns `None` when either file is unreadable, the certificate chain is
/// empty, the private key cannot be parsed, or rustls rejects the key — the
/// caller then fails the connect, matching PHP's behavior when `ssl.local_cert`
/// cannot be loaded. Encrypted (passphrase-protected) keys are NOT supported:
/// `rustls-pemfile` only reads unencrypted PEM keys, so an encrypted key yields
/// `None` (the `ssl.passphrase` option cannot decrypt it in the rustls subset).
/// Not cached: client-cert connects are rare and each may use a distinct key.
fn client_cert_config(cert_path: &str, key_path: &str) -> Option<Arc<ClientConfig>> {
    let _ = rustls::crypto::ring::default_provider().install_default();
    let cert_pem = std::fs::read(cert_path).ok()?;
    let mut cert_reader: &[u8] = &cert_pem;
    let certs: Vec<CertificateDer<'static>> = rustls_pemfile::certs(&mut cert_reader)
        .filter_map(|c| c.ok())
        .collect();
    if certs.is_empty() {
        return None;
    }
    let key_pem = std::fs::read(key_path).ok()?;
    let mut key_reader: &[u8] = &key_pem;
    let key: PrivateKeyDer<'static> = rustls_pemfile::private_key(&mut key_reader).ok().flatten()?;
    let mut roots = RootCertStore::empty();
    roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
    ClientConfig::builder()
        .with_root_certificates(roots)
        .with_client_auth_cert(certs, key)
        .ok()
        .map(Arc::new)
}

/// Variant of `elephc_tls_connect` that presents a client certificate (mutual
/// TLS) loaded from the `ssl.local_cert` / `ssl.local_pk` PEM files. Returns an
/// integer handle ID, or `-1` on any failure (including an unreadable/malformed
/// cert or key). The non-client-cert connect variants ignore the trailing
/// `cert_*`/`key_*` arguments, so the runtime can share one call site and just
/// select the function pointer.
///
/// # Safety
///
/// `host_ptr`/`cert_ptr`/`key_ptr` must point to `host_len`/`cert_len`/`key_len`
/// valid UTF-8 bytes for the duration of this call.
#[no_mangle]
pub unsafe extern "C" fn elephc_tls_connect_client_cert(
    host_ptr: *const u8,
    host_len: usize,
    port: u16,
    cert_ptr: *const u8,
    cert_len: usize,
    key_ptr: *const u8,
    key_len: usize,
) -> i64 {
    let Some((cert_path, key_path)) = client_cert_paths(cert_ptr, cert_len, key_ptr, key_len) else {
        return -1;
    };
    let config = match client_cert_config(cert_path, key_path) {
        Some(c) => c,
        None => return -1,
    };
    tls_connect_inner(host_ptr, host_len, port, config)
}

/// Variant of `elephc_tls_attach_fd` that presents a client certificate (mutual
/// TLS) loaded from the `ssl.local_cert` / `ssl.local_pk` PEM files when
/// promoting an already-connected TCP fd to TLS. Returns a handle ID, or `-1`
/// on dup / handshake / SNI / cert-load failure. Used by
/// `stream_socket_enable_crypto` when the active stream context carries a
/// `local_cert`.
///
/// # Safety
///
/// `fd` must refer to a connected TCP socket owned by the caller.
/// `peer_name_ptr`/`cert_ptr`/`key_ptr` must point to their respective lengths
/// of valid UTF-8 bytes for the duration of this call.
#[no_mangle]
pub unsafe extern "C" fn elephc_tls_attach_fd_client_cert(
    fd: i32,
    peer_name_ptr: *const u8,
    peer_name_len: usize,
    cert_ptr: *const u8,
    cert_len: usize,
    key_ptr: *const u8,
    key_len: usize,
) -> i64 {
    if fd < 0 || peer_name_ptr.is_null() || peer_name_len == 0 {
        return -1;
    }
    let Some((cert_path, key_path)) = client_cert_paths(cert_ptr, cert_len, key_ptr, key_len) else {
        return -1;
    };
    let config = match client_cert_config(cert_path, key_path) {
        Some(c) => c,
        None => return -1,
    };
    let host_bytes = std::slice::from_raw_parts(peer_name_ptr, peer_name_len);
    let host = match std::str::from_utf8(host_bytes) {
        Ok(s) => s,
        Err(_) => return -1,
    };
    let server_name: ServerName<'static> = match ServerName::try_from(host.to_string()) {
        Ok(name) => name,
        Err(_) => return -1,
    };
    let dup_fd = libc::dup(fd);
    if dup_fd < 0 {
        return -1;
    }
    let sock = TcpStream::from_raw_fd(dup_fd);
    let conn = match ClientConnection::new(config, server_name) {
        Ok(c) => c,
        Err(_) => return -1,
    };
    let id = next_handle_id();
    handles()
        .lock()
        .unwrap()
        .insert(id, Box::new(HandleEntry { sock, conn }));
    id
}

/// Validates and decodes the client-cert/key pointer pair into `&str` paths.
/// Returns `None` when either pointer is null/empty or not valid UTF-8.
///
/// # Safety
///
/// The pointers, when non-null, must reference the given lengths of bytes for
/// the duration of this call.
unsafe fn client_cert_paths<'a>(
    cert_ptr: *const u8,
    cert_len: usize,
    key_ptr: *const u8,
    key_len: usize,
) -> Option<(&'a str, &'a str)> {
    if cert_ptr.is_null() || cert_len == 0 || key_ptr.is_null() || key_len == 0 {
        return None;
    }
    let cert_path = std::str::from_utf8(std::slice::from_raw_parts(cert_ptr, cert_len)).ok()?;
    let key_path = std::str::from_utf8(std::slice::from_raw_parts(key_ptr, key_len)).ok()?;
    Some((cert_path, key_path))
}

unsafe fn tls_connect_inner(
    host_ptr: *const u8,
    host_len: usize,
    port: u16,
    config: Arc<ClientConfig>,
) -> i64 {
    tls_connect_inner_named(host_ptr, host_len, port, config, None)
}

/// Like `tls_connect_inner` but verifies the certificate (and sends SNI) for
/// `peer_name` when it is `Some`, while still opening the TCP connection to the
/// real `host`. With `None` the certificate is verified for `host`.
unsafe fn tls_connect_inner_named(
    host_ptr: *const u8,
    host_len: usize,
    port: u16,
    config: Arc<ClientConfig>,
    peer_name: Option<&str>,
) -> i64 {
    if host_ptr.is_null() || host_len == 0 {
        return -1;
    }
    let host_bytes = std::slice::from_raw_parts(host_ptr, host_len);
    let host = match std::str::from_utf8(host_bytes) {
        Ok(s) => s,
        Err(_) => return -1,
    };
    // The certificate is checked against peer_name when provided, else host.
    let verify_name = peer_name.unwrap_or(host);
    let server_name: ServerName<'static> = match ServerName::try_from(verify_name.to_string()) {
        Ok(name) => name,
        Err(_) => return -1,
    };
    let sock = match TcpStream::connect((host, port)) {
        Ok(s) => s,
        Err(_) => return -1,
    };
    let conn = match ClientConnection::new(config, server_name) {
        Ok(c) => c,
        Err(_) => return -1,
    };
    let id = next_handle_id();
    handles()
        .lock()
        .unwrap()
        .insert(id, Box::new(HandleEntry { sock, conn }));
    id
}

/// Read up to `max_len` decrypted bytes from the TLS session into `buf_ptr`.
/// Returns the byte count, `0` on EOF, or `-1` on error / unknown handle.
///
/// # Safety
///
/// `buf_ptr` must be writable for `max_len` bytes.
#[no_mangle]
pub unsafe extern "C" fn elephc_tls_read(
    handle_id: i64,
    buf_ptr: *mut u8,
    max_len: usize,
) -> isize {
    if buf_ptr.is_null() || max_len == 0 {
        return 0;
    }
    let mut guard = handles().lock().unwrap();
    let Some(entry) = guard.get_mut(&handle_id) else {
        return -1;
    };
    let buf = std::slice::from_raw_parts_mut(buf_ptr, max_len);
    let mut stream = Stream::new(&mut entry.conn, &mut entry.sock);
    match stream.read(buf) {
        Ok(n) => n as isize,
        Err(_) => -1,
    }
}

/// Encrypt and send `len` bytes from `buf_ptr` over the TLS session. Returns
/// the byte count written, or `-1` on error / unknown handle.
///
/// # Safety
///
/// `buf_ptr` must be readable for `len` bytes.
#[no_mangle]
pub unsafe extern "C" fn elephc_tls_write(
    handle_id: i64,
    buf_ptr: *const u8,
    len: usize,
) -> isize {
    if buf_ptr.is_null() || len == 0 {
        return 0;
    }
    let mut guard = handles().lock().unwrap();
    let Some(entry) = guard.get_mut(&handle_id) else {
        return -1;
    };
    let buf = std::slice::from_raw_parts(buf_ptr, len);
    let mut stream = Stream::new(&mut entry.conn, &mut entry.sock);
    match stream.write(buf) {
        Ok(n) => n as isize,
        Err(_) => -1,
    }
}

/// Attach a TLS session to an already-connected TCP file descriptor.
/// Used by `stream_socket_enable_crypto` to promote an existing
/// `stream_socket_client("tcp://...")` socket to TLS without
/// re-establishing the TCP connection.
///
/// The wrapper duplicates `fd` via `libc::dup` so it owns its own
/// reference for the rustls `TcpStream`. The caller's original `fd`
/// remains valid but must not be used for I/O while the TLS session is
/// live — encrypted reads/writes would race with raw reads on the same
/// socket. The elephc runtime routes subsequent fread/fwrite/fclose
/// through the TLS handle instead of the bare fd.
///
/// Returns a handle ID, or `-1` on dup / handshake / SNI failure.
///
/// # Safety
///
/// `fd` must refer to a connected TCP socket owned by the caller.
/// `peer_name_ptr` must point to `peer_name_len` valid UTF-8 bytes for
/// the duration of this call; the peer name is used both for SNI and
/// for certificate-name validation by rustls.
#[no_mangle]
pub unsafe extern "C" fn elephc_tls_attach_fd(
    fd: i32,
    peer_name_ptr: *const u8,
    peer_name_len: usize,
) -> i64 {
    if fd < 0 || peer_name_ptr.is_null() || peer_name_len == 0 {
        return -1;
    }
    let host_bytes = std::slice::from_raw_parts(peer_name_ptr, peer_name_len);
    let host = match std::str::from_utf8(host_bytes) {
        Ok(s) => s,
        Err(_) => return -1,
    };
    let server_name: ServerName<'static> = match ServerName::try_from(host.to_string()) {
        Ok(name) => name,
        Err(_) => return -1,
    };
    // dup so the rustls Stream owns its own file descriptor; closing the
    // TLS handle (via elephc_tls_close) will drop the dup, leaving the
    // original fd intact for the caller's fclose path.
    let dup_fd = libc::dup(fd);
    if dup_fd < 0 {
        return -1;
    }
    let sock = TcpStream::from_raw_fd(dup_fd);
    let conn = match ClientConnection::new(shared_client_config(), server_name) {
        Ok(c) => c,
        Err(_) => {
            // sock drops here and closes the dup'd fd.
            return -1;
        }
    };
    let id = next_handle_id();
    handles()
        .lock()
        .unwrap()
        .insert(id, Box::new(HandleEntry { sock, conn }));
    id
}

/// Send a TLS close_notify, drop the underlying socket, and remove the
/// session from the handle table.
#[no_mangle]
pub extern "C" fn elephc_tls_close(handle_id: i64) {
    let mut guard = handles().lock().unwrap();
    if let Some(mut entry) = guard.remove(&handle_id) {
        entry.conn.send_close_notify();
        let mut stream = Stream::new(&mut entry.conn, &mut entry.sock);
        let _ = stream.flush();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_is_v1() {
        assert_eq!(elephc_tls_version(), 1);
    }

    #[test]
    fn unknown_handle_read_returns_minus_one() {
        let mut buf = [0u8; 16];
        let n = unsafe { elephc_tls_read(0xDEAD_BEEF, buf.as_mut_ptr(), buf.len()) };
        assert_eq!(n, -1);
    }

    #[test]
    fn unknown_handle_write_returns_minus_one() {
        let buf = [0u8; 4];
        let n = unsafe { elephc_tls_write(0xDEAD_BEEF, buf.as_ptr(), buf.len()) };
        assert_eq!(n, -1);
    }

    #[test]
    fn close_unknown_handle_is_no_op() {
        // Should not panic when the handle is not in the table.
        elephc_tls_close(0xDEAD_BEEF);
    }

    /// `cafile_client_config` must reject a missing path and PEM bytes that
    /// contain no certificates, so a bad `ssl.cafile` fails the connect rather
    /// than silently trusting nothing or panicking.
    #[test]
    fn cafile_config_rejects_missing_and_certless() {
        assert!(cafile_client_config("/nonexistent/elephc/ca-bundle.pem").is_none());

        let mut path = std::env::temp_dir();
        path.push(format!("elephc_cafile_test_{}.pem", std::process::id()));
        std::fs::write(&path, b"not a certificate, just noise\n").unwrap();
        let result = cafile_client_config(path.to_str().unwrap());
        let _ = std::fs::remove_file(&path);
        assert!(result.is_none(), "a cert-less PEM must yield no config");
    }

    /// A bogus cafile path passed through the C entry point returns the `-1`
    /// failure handle (no network access, since the cafile load fails first).
    #[test]
    fn connect_cafile_bad_path_returns_minus_one() {
        let host = "127.0.0.1";
        let cafile = "/nonexistent/elephc/ca-bundle.pem";
        let id = unsafe {
            elephc_tls_connect_cafile(
                host.as_ptr(),
                host.len(),
                9,
                cafile.as_ptr(),
                cafile.len(),
            )
        };
        assert_eq!(id, -1);
    }

    /// `capath_client_config` must reject a missing directory and a directory
    /// that holds no certificates, so a bad `ssl.capath` fails the connect
    /// rather than silently trusting nothing or panicking.
    #[test]
    fn capath_config_rejects_missing_and_certless() {
        assert!(capath_client_config("/nonexistent/elephc/ca-dir").is_none());

        let mut dir = std::env::temp_dir();
        dir.push(format!("elephc_capath_test_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("noise.txt"), b"not a certificate\n").unwrap();
        let result = capath_client_config(dir.to_str().unwrap());
        let _ = std::fs::remove_dir_all(&dir);
        assert!(result.is_none(), "a cert-less directory must yield no config");
    }

    /// A bogus capath directory passed through the C entry point returns the
    /// `-1` failure handle (the capath scan fails before any network access).
    #[test]
    fn connect_capath_bad_path_returns_minus_one() {
        let host = "127.0.0.1";
        let capath = "/nonexistent/elephc/ca-dir";
        let id = unsafe {
            elephc_tls_connect_capath(host.as_ptr(), host.len(), 9, capath.as_ptr(), capath.len())
        };
        assert_eq!(id, -1);
    }

    /// `elephc_tls_connect_peer_name` to an unreachable port returns `-1`: the
    /// TCP connect fails fast, exercising the peer-name entry point without a
    /// live TLS server.
    #[test]
    fn connect_peer_name_unreachable_returns_minus_one() {
        let host = "127.0.0.1";
        let peer = "example.com";
        let id = unsafe {
            elephc_tls_connect_peer_name(host.as_ptr(), host.len(), 9, peer.as_ptr(), peer.len())
        };
        assert_eq!(id, -1);
    }

    /// End-to-end TLS smoke against a real HTTPS host. Requires outbound
    /// network access, so it is `#[ignore]`d by default; run with
    /// `cargo test -p elephc-tls -- --ignored` to exercise it.
    #[test]
    #[ignore]
    fn real_https_roundtrip_against_example_com() {
        let host = "example.com";
        let id = unsafe {
            elephc_tls_connect(host.as_ptr(), host.len(), 443)
        };
        assert!(id > 0, "TLS connect failed");

        let req = b"GET / HTTP/1.0\r\nHost: example.com\r\nConnection: close\r\n\r\n";
        let n = unsafe { elephc_tls_write(id, req.as_ptr(), req.len()) };
        assert!(n > 0, "TLS write failed");

        let mut buf = vec![0u8; 4096];
        let mut total = 0usize;
        loop {
            let r = unsafe {
                elephc_tls_read(id, buf.as_mut_ptr().add(0), buf.len())
            };
            if r <= 0 {
                break;
            }
            total += r as usize;
            if total >= 200 {
                break;
            }
        }
        elephc_tls_close(id);

        assert!(total >= 12, "expected at least an HTTP status line; got {} bytes", total);
        let head = std::str::from_utf8(&buf[..total.min(buf.len())]).unwrap_or("");
        assert!(
            head.starts_with("HTTP/"),
            "expected HTTP/* status line, got: {:?}",
            &head[..head.len().min(40)],
        );
    }

    /// A throwaway self-signed certificate and its unencrypted PKCS#8 key, used
    /// to exercise the `ssl.local_cert` / `ssl.local_pk` client-auth config path
    /// without a network round-trip. Generated with
    /// `openssl req -x509 -newkey rsa:2048 -nodes -subj /CN=elephc-test`.
    const TEST_CLIENT_CERT_PEM: &str = "\
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

    const TEST_CLIENT_KEY_PEM: &str = "\
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

    /// `client_cert_config` builds a usable mutual-TLS `ClientConfig` from a
    /// valid PEM certificate chain and unencrypted private key — the
    /// `ssl.local_cert` / `ssl.local_pk` happy path.
    #[test]
    fn client_cert_config_builds_from_valid_cert_and_key() {
        let dir = std::env::temp_dir();
        let cert = dir.join(format!("elephc_cc_cert_{}.pem", std::process::id()));
        let key = dir.join(format!("elephc_cc_key_{}.pem", std::process::id()));
        std::fs::write(&cert, TEST_CLIENT_CERT_PEM).unwrap();
        std::fs::write(&key, TEST_CLIENT_KEY_PEM).unwrap();
        let result = client_cert_config(cert.to_str().unwrap(), key.to_str().unwrap());
        let _ = std::fs::remove_file(&cert);
        let _ = std::fs::remove_file(&key);
        assert!(result.is_some(), "a valid cert+key must yield a client-auth config");
    }

    /// `client_cert_config` returns `None` for a missing cert, a missing key,
    /// and a cert-less PEM, so a bad `ssl.local_cert`/`ssl.local_pk` fails the
    /// connect rather than silently connecting without client auth.
    #[test]
    fn client_cert_config_rejects_missing_and_certless() {
        assert!(client_cert_config("/nonexistent/cert.pem", "/nonexistent/key.pem").is_none());

        let dir = std::env::temp_dir();
        let cert = dir.join(format!("elephc_cc_bad_cert_{}.pem", std::process::id()));
        std::fs::write(&cert, b"not a certificate\n").unwrap();
        let key = dir.join(format!("elephc_cc_bad_key_{}.pem", std::process::id()));
        std::fs::write(&key, TEST_CLIENT_KEY_PEM).unwrap();
        // valid key but cert-less cert file → None
        let certless = client_cert_config(cert.to_str().unwrap(), key.to_str().unwrap());
        // valid cert but missing key file → None
        std::fs::write(&cert, TEST_CLIENT_CERT_PEM).unwrap();
        let no_key = client_cert_config(cert.to_str().unwrap(), "/nonexistent/key.pem");
        let _ = std::fs::remove_file(&cert);
        let _ = std::fs::remove_file(&key);
        assert!(certless.is_none(), "a cert-less cert file must yield no config");
        assert!(no_key.is_none(), "a missing key file must yield no config");
    }

    /// A bogus client-cert path passed through the C entry point returns the
    /// `-1` failure handle (the cert load fails before any network access).
    #[test]
    fn connect_client_cert_bad_path_returns_minus_one() {
        let host = "127.0.0.1";
        let cert = "/nonexistent/cert.pem";
        let key = "/nonexistent/key.pem";
        let id = unsafe {
            elephc_tls_connect_client_cert(
                host.as_ptr(),
                host.len(),
                9,
                cert.as_ptr(),
                cert.len(),
                key.as_ptr(),
                key.len(),
            )
        };
        assert_eq!(id, -1);
    }

    /// `elephc_tls_attach_fd_client_cert` rejects a null cert/key pointer pair
    /// with `-1` before touching the (here invalid) fd.
    #[test]
    fn attach_client_cert_null_paths_returns_minus_one() {
        let peer = "example.com";
        let id = unsafe {
            elephc_tls_attach_fd_client_cert(
                3,
                peer.as_ptr(),
                peer.len(),
                std::ptr::null(),
                0,
                std::ptr::null(),
                0,
            )
        };
        assert_eq!(id, -1);
    }
}
