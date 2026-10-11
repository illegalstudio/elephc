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
//! - Windows trust anchors come from the native certificate store, matching
//!   php-src; other targets retain the deterministic `webpki-roots` bundle.
//! - The v2 handshake ABI returns `1` when authentication is complete, `0`
//!   when a nonblocking socket needs more I/O, and `-1` on a terminal error.
//!   Read/write use `-1` for terminal failures, `-2` for `WouldBlock`, and
//!   `-3` for `TimedOut`; the handle remains installed while a handshake is
//!   in progress.
//! - The v3 option ABI passes one fixed-layout 88-byte block to connect/attach,
//!   keeping verification policy, trust sources, peer name, and client identity
//!   combinable without proliferating mutually exclusive exports.

use std::collections::HashMap;
use std::io::{Read, Write};
#[cfg(windows)]
use std::mem::ManuallyDrop;
use std::net::TcpStream;
#[cfg(unix)]
use std::os::fd::FromRawFd;
#[cfg(windows)]
use std::os::windows::io::{FromRawSocket, RawSocket};
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use aes::{Aes128, Aes192, Aes256};
use base64::Engine;
use cbc::cipher::{block_padding::Pkcs7, BlockDecryptMut, KeyIvInit};
use des::{Des, TdesEde2, TdesEde3};
use rustls::client::danger::{HandshakeSignatureValid, ServerCertVerified, ServerCertVerifier};
use rustls::client::{verify_server_cert_signed_by_trust_anchor, verify_server_name};
use rustls::crypto::{
    CryptoProvider, WebPkiSupportedAlgorithms, verify_tls12_signature, verify_tls13_signature,
};
use rustls::pki_types::{
    CertificateDer, PrivateKeyDer, PrivatePkcs1KeyDer, PrivatePkcs8KeyDer,
    PrivateSec1KeyDer, ServerName, UnixTime,
};
use rustls::server::{ParsedCertificate, ServerConfig, ServerConnection};
use rustls::{
    ClientConfig, ClientConnection, Connection, DigitallySignedStruct, Error as RustlsError,
    CipherSuite, RootCertStore, SignatureScheme, Stream, SupportedProtocolVersion,
};
use md5::Md5;
use sha1::Sha1;
use sha2::{Digest, Sha256};
use zeroize::{Zeroize, Zeroizing};

/// TLS v2 result for an unrecoverable bridge, socket, or TLS error.
const TLS_IO_TERMINAL: isize = -1;

/// TLS v2 result for an operation that can be retried without EOF.
const TLS_IO_WOULD_BLOCK: isize = -2;

/// TLS v2 result for an operation that reached its configured socket timeout.
const TLS_IO_TIMED_OUT: isize = -3;

/// Verifies that the server certificate chains to a configured trust anchor.
pub const TLS_VERIFY_PEER: u32 = 1;

/// Verifies that the server certificate matches the requested peer name.
pub const TLS_VERIFY_PEER_NAME: u32 = 2;

/// Permits a genuinely self-signed depth-zero leaf when chain verification fails.
pub const TLS_ALLOW_SELF_SIGNED: u32 = 4;

/// PHP-compatible secure default: validate both the chain and peer name.
pub const TLS_DEFAULT_VERIFY_FLAGS: u32 = TLS_VERIFY_PEER | TLS_VERIFY_PEER_NAME;

/// Version of the fixed-layout `ElephcTlsClientOptions` C ABI.
pub const TLS_CLIENT_OPTIONS_ABI_VERSION: u32 = 6;

/// Previous option-block ABI accepted for compatibility with older generated
/// callers. It carries the original 88-byte client-only layout.
const TLS_CLIENT_OPTIONS_ABI_VERSION_1: u32 = 1;
const TLS_CLIENT_OPTIONS_ABI_VERSION_3: u32 = 3;
const TLS_CLIENT_OPTIONS_ABI_VERSION_4: u32 = 4;
const TLS_CLIENT_OPTIONS_ABI_VERSION_5: u32 = 5;

/// Extended TLS option flags carried by ABI v5 and v6.
const TLS_OPTION_SNI_DISABLED: u32 = 1 << 0;
const TLS_OPTION_NO_TICKET: u32 = 1 << 1;

/// Mask of every verification option accepted by the bridge ABI.
const TLS_KNOWN_VERIFY_FLAGS: u32 =
    TLS_VERIFY_PEER | TLS_VERIFY_PEER_NAME | TLS_ALLOW_SELF_SIGNED;

/// PHP stream-crypto bit selecting client rather than server mode.
const PHP_STREAM_CRYPTO_IS_CLIENT: i64 = 1;

/// PHP stream-crypto bit selecting TLS 1.2.
const PHP_STREAM_CRYPTO_TLS_1_2: i64 = 32;

/// PHP stream-crypto bit selecting TLS 1.3.
const PHP_STREAM_CRYPTO_TLS_1_3: i64 = 64;

/// PHP's default aggregate TLS client method.
const PHP_STREAM_CRYPTO_TLS_CLIENT: i64 = 121;

/// Every method bit currently defined by PHP's stream crypto API.
const PHP_STREAM_CRYPTO_KNOWN_BITS: i64 = 127;

/// rustls protocol list for a TLS 1.2-only request.
const TLS_1_2_ONLY: &[&SupportedProtocolVersion] = &[&rustls::version::TLS12];

/// rustls protocol list for a TLS 1.3-only request.
const TLS_1_3_ONLY: &[&SupportedProtocolVersion] = &[&rustls::version::TLS13];

/// rustls protocol list for the ordinary TLS client aggregate.
const TLS_1_2_AND_1_3: &[&SupportedProtocolVersion] =
    &[&rustls::version::TLS13, &rustls::version::TLS12];

/// Winsock `SOL_SOCKET` level used to query socket-wide options.
#[cfg(windows)]
const WINDOWS_SOL_SOCKET: libc::c_int = 0xffff;

/// Winsock `SO_TYPE` option used to distinguish TCP from datagram sockets.
#[cfg(windows)]
const WINDOWS_SO_TYPE: libc::c_int = 0x1008;

/// Winsock stream-socket type required before adopting a socket as TCP.
#[cfg(windows)]
const WINDOWS_SOCK_STREAM: libc::c_int = 1;

/// Winsock IPv4 address family accepted by `TcpStream`.
#[cfg(windows)]
const WINDOWS_AF_INET: u16 = 2;

/// Winsock IPv6 address family accepted by `TcpStream`.
#[cfg(windows)]
const WINDOWS_AF_INET6: u16 = 23;

/// Converts a Rust I/O error into the stable TLS v2 read/write ABI sentinel.
fn tls_io_error_result(error: &std::io::Error) -> isize {
    match error.kind() {
        std::io::ErrorKind::WouldBlock => TLS_IO_WOULD_BLOCK,
        std::io::ErrorKind::TimedOut => TLS_IO_TIMED_OUT,
        _ => TLS_IO_TERMINAL,
    }
}

/// Converts a Rust TLS read error into the stable read-side ABI result.
///
/// php-src enables OpenSSL's `SSL_OP_IGNORE_UNEXPECTED_EOF` for stream
/// clients. Rustls reports the equivalent peer TCP close without a
/// `close_notify` alert as an error, but PHP surfaces the bytes already read
/// and marks the stream at EOF. Writes retain the terminal-error mapping.
fn tls_read_error_result(error: &std::io::Error) -> isize {
    if error.kind() == std::io::ErrorKind::UnexpectedEof {
        0
    } else {
        tls_io_error_result(error)
    }
}

/// Returns lowercase hexadecimal bytes without allocating an intermediate
/// formatter. PHP compares peer fingerprints case-insensitively.
fn hex_digest(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);
    for &byte in bytes {
        output.push(HEX[(byte >> 4) as usize] as char);
        output.push(HEX[(byte & 0x0f) as usize] as char);
    }
    output
}

/// Matches PHP's scalar or algorithm-keyed fingerprint forms after a completed
/// TLS handshake.
///
/// PHP 8.5 selects MD5, SHA-1, or SHA-256 from the canonical hexadecimal
/// fingerprint length. Malformed values fail closed before application data is
/// exposed.
fn peer_fingerprint_matches(conn: &Connection, expected: &str) -> bool {
    if expected.contains('=') {
        let mut entries = 0;
        for entry in expected.split(';') {
            let Some((algorithm, value)) = entry.split_once('=') else {
                return false;
            };
            let normalized: String = value
                .bytes()
                .filter(|byte| *byte != b':')
                .map(|byte| byte.to_ascii_lowercase() as char)
                .collect();
            let algorithm = algorithm.to_ascii_lowercase();
            if !matches!(algorithm.as_str(), "md5" | "sha1" | "sha256")
                || !fingerprint_value_matches_certificate_for_algorithm(
                    conn,
                    &normalized,
                    &algorithm,
                )
            {
                return false;
            }
            entries += 1;
        }
        return entries != 0;
    }
    let normalized: String = expected
        .bytes()
        .filter(|byte| *byte != b':')
        .map(|byte| byte.to_ascii_lowercase() as char)
        .collect();
    let digest = conn
        .peer_certificates()
        .and_then(|certificates| certificates.first())
        .map(|certificate| certificate.as_ref());
    digest.is_some_and(|certificate| fingerprint_value_matches_certificate(certificate, &normalized, None))
}

fn fingerprint_value_matches_certificate_for_algorithm(
    conn: &Connection,
    normalized: &str,
    algorithm: &str,
) -> bool {
    conn.peer_certificates()
        .and_then(|certificates| certificates.first())
        .is_some_and(|certificate| {
            fingerprint_value_matches_certificate(certificate.as_ref(), normalized, Some(algorithm))
        })
}

/// Compares one normalized PHP fingerprint value against a DER certificate.
fn fingerprint_value_matches_certificate(
    certificate: &[u8],
    normalized: &str,
    algorithm: Option<&str>,
) -> bool {
    let actual = match (algorithm, normalized.len()) {
        (Some("md5"), 32) | (None, 32) => hex_digest(&Md5::digest(certificate)),
        (Some("sha1"), 40) | (None, 40) => hex_digest(&Sha1::digest(certificate)),
        (Some("sha256"), 64) | (None, 64) => hex_digest(&Sha256::digest(certificate)),
        _ => return false,
    };
    actual == normalized
}

/// Enforces the optional fingerprint after rustls has authenticated the peer.
fn verify_entry_fingerprint(entry: &HandleEntry) -> bool {
    entry
        .peer_fingerprint
        .as_deref()
        .is_none_or(|expected| peer_fingerprint_matches(&entry.conn, expected))
}

/// Returns the elephc-tls ABI version.
///
/// v2 adds `elephc_tls_handshake`, whose tri-state result lets PHP's
/// `stream_socket_enable_crypto()` distinguish completion, nonblocking
/// progress, and failure without discarding the live rustls connection.
/// v3 adds the fixed-layout option block shared by connect and attach; v6
/// appends the strict cipher/security/compression context tail.
#[no_mangle]
pub extern "C" fn elephc_tls_version() -> i32 {
    TLS_CLIENT_OPTIONS_ABI_VERSION as i32
}

struct HandleEntry {
    sock: TcpStream,
    conn: Connection,
    config: Option<Arc<ClientConfig>>,
    peer_fingerprint: Option<String>,
    fingerprint_checked: bool,
}

/// Returns the process-wide TLS handle table guarded by a mutex.
fn handles() -> &'static Mutex<HashMap<i64, Box<HandleEntry>>> {
    static HANDLES: OnceLock<Mutex<HashMap<i64, Box<HandleEntry>>>> = OnceLock::new();
    HANDLES.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Allocates the next positive handle id for a TLS session.
fn next_handle_id() -> i64 {
    static NEXT_ID: AtomicI64 = AtomicI64::new(1);
    NEXT_ID.fetch_add(1, Ordering::SeqCst)
}

/// Inserts one live TLS stream into the process handle table.
///
/// A prior panic leaves the mutex marked poisoned but does not invalidate the
/// table contents. Recover its inner map so this C ABI remains usable rather
/// than turning one contained failure into a permanent TLS outage.
fn insert_handle(
    sock: TcpStream,
    conn: Connection,
    config: Arc<ClientConfig>,
    peer_fingerprint: Option<&str>,
) -> i64 {
    let id = next_handle_id();
    let mut guard = handles()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    guard.insert(
        id,
        Box::new(HandleEntry {
            sock,
            conn,
            config: Some(config),
            peer_fingerprint: peer_fingerprint.map(str::to_owned),
            fingerprint_checked: false,
        }),
    );
    id
}

/// Inserts one server-side session into the shared descriptor/session table.
///
/// The existing table retains an optional client configuration for session
/// resumption. Server sessions deliberately retain no client config, so they
/// cannot be accidentally reused as a client session.
fn insert_server_handle(
    sock: TcpStream,
    conn: ServerConnection,
    peer_fingerprint: Option<&str>,
) -> i64 {
    let id = next_handle_id();
    let mut guard = handles()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    guard.insert(
        id,
        Box::new(HandleEntry {
            sock,
            conn: Connection::Server(conn),
            config: None,
            peer_fingerprint: peer_fingerprint.map(str::to_owned),
            fingerprint_checked: false,
        }),
    );
    id
}

/// Clones the rustls client configuration retained by one live source session.
///
/// Reusing the same `Arc<ClientConfig>` also shares rustls's client-session
/// cache, mirroring php-src's SSL context and session reuse for
/// `stream_socket_enable_crypto(..., session_stream: $source)`.
fn session_client_config(handle_id: i64) -> Option<Arc<ClientConfig>> {
    if handle_id <= 0 {
        return None;
    }
    let guard = handles().lock().ok()?;
    guard.get(&handle_id)?.config.clone()
}

/// Independent PHP-compatible peer-verification switches decoded from the C ABI.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
struct VerificationPolicy {
    verify_peer: bool,
    verify_peer_name: bool,
    allow_self_signed: bool,
}

impl VerificationPolicy {
    /// Decodes the stable verification bitset, rejecting unknown future bits.
    fn from_flags(flags: u32) -> Option<Self> {
        if flags & !TLS_KNOWN_VERIFY_FLAGS != 0 {
            return None;
        }
        Some(Self {
            verify_peer: flags & TLS_VERIFY_PEER != 0,
            verify_peer_name: flags & TLS_VERIFY_PEER_NAME != 0,
            allow_self_signed: flags & TLS_ALLOW_SELF_SIGNED != 0,
        })
    }
}

/// Parsed TLS configuration shared by connect and existing-socket attachment.
///
/// The v6 compiler ABI carries peer/trust/client-identity fields plus ALPN,
/// SNI/ticket flags, a peer fingerprint, an encrypted-key passphrase, and the
/// fail-closed cipher/security/compression policy fields. Omitted options stay
/// distinct from explicit zero/false values.
#[derive(Clone, Copy, Debug)]
struct TlsOptions<'a> {
    verification_flags: u32,
    cafile: Option<&'a str>,
    capath: Option<&'a str>,
    client_cert: Option<&'a str>,
    client_key: Option<&'a str>,
    passphrase: Option<&'a str>,
    alpn_protocols: Option<&'a str>,
    peer_fingerprint: Option<&'a str>,
    /// OpenSSL cipher-list syntax reduced to the exact rustls suite subset.
    ciphers: Option<&'a str>,
    /// OpenSSL security level. Only the rustls default level is representable.
    security_level: Option<i64>,
    /// `Some(false)` is rejected because rustls has no TLS compression support.
    disable_compression: Option<bool>,
    option_flags: u32,
}

/// Fixed-layout TLS option block shared by connect and existing-socket attach.
///
/// ABI v6 is 184 bytes: two `u32` header fields followed by nine
/// pointer/length pairs and one `u32` option flag word. ABI v3 callers are
/// still accepted by the decoder and leave the appended fields absent. The
/// v6 scalar presence words distinguish explicit zero/false values from an
/// omitted PHP option.
/// Server methods reuse the certificate/key pair as `local_cert`/`local_pk`.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct ElephcTlsClientOptions {
    /// Must equal `TLS_CLIENT_OPTIONS_ABI_VERSION`.
    pub abi_version: u32,
    /// Bitset composed from `TLS_VERIFY_*` and `TLS_ALLOW_SELF_SIGNED`.
    pub verification_flags: u32,
    /// Optional SNI and certificate-name override.
    pub peer_name_ptr: *const u8,
    /// Byte length of `peer_name_ptr`.
    pub peer_name_len: usize,
    /// Optional PEM CA bundle path.
    pub cafile_ptr: *const u8,
    /// Byte length of `cafile_ptr`.
    pub cafile_len: usize,
    /// Optional CA-directory path.
    pub capath_ptr: *const u8,
    /// Byte length of `capath_ptr`.
    pub capath_len: usize,
    /// Optional PEM client certificate path.
    pub cert_ptr: *const u8,
    /// Byte length of `cert_ptr`.
    pub cert_len: usize,
    /// Optional PEM client private-key path.
    pub key_ptr: *const u8,
    /// Byte length of `key_ptr`.
    pub key_len: usize,
    /// Optional comma-separated ALPN protocol list.
    pub alpn_protocols_ptr: *const u8,
    /// Byte length of `alpn_protocols_ptr`.
    pub alpn_protocols_len: usize,
    /// Optional PHP peer fingerprint, scalar or `algorithm=fingerprint;...` list.
    pub peer_fingerprint_ptr: *const u8,
    /// Byte length of `peer_fingerprint_ptr`.
    pub peer_fingerprint_len: usize,
    /// `TLS_OPTION_*` flags.
    pub option_flags: u32,
    /// ABI padding reserved for future fields.
    pub _reserved: u32,
    /// Optional passphrase for an encrypted PKCS#8 private key.
    pub passphrase_ptr: *const u8,
    /// Byte length of `passphrase_ptr`.
    pub passphrase_len: usize,
    /// Optional OpenSSL cipher list. Only the exact rustls suite subset is accepted.
    pub ciphers_ptr: *const u8,
    /// Byte length of `ciphers_ptr`.
    pub ciphers_len: usize,
    /// Requested OpenSSL security level.
    pub security_level: i64,
    /// Nonzero when `security_level` was explicitly supplied.
    pub security_level_present: u32,
    /// PHP `disable_compression` truth value, when explicitly supplied.
    pub disable_compression: u32,
    /// Nonzero when `disable_compression` was explicitly supplied.
    pub disable_compression_present: u32,
}

/// ABI v3 prefix retained so an older generated caller cannot make the v6
/// decoder read beyond its 88-byte allocation.
#[repr(C)]
#[derive(Clone, Copy)]
struct ElephcTlsClientOptionsV3 {
    abi_version: u32,
    verification_flags: u32,
    peer_name_ptr: *const u8,
    peer_name_len: usize,
    cafile_ptr: *const u8,
    cafile_len: usize,
    capath_ptr: *const u8,
    capath_len: usize,
    cert_ptr: *const u8,
    cert_len: usize,
    key_ptr: *const u8,
    key_len: usize,
}

/// Exact ABI v4 layout retained for callers that allocated only 128 bytes.
#[repr(C)]
#[derive(Clone, Copy)]
struct ElephcTlsClientOptionsV4 {
    abi_version: u32,
    verification_flags: u32,
    peer_name_ptr: *const u8,
    peer_name_len: usize,
    cafile_ptr: *const u8,
    cafile_len: usize,
    capath_ptr: *const u8,
    capath_len: usize,
    cert_ptr: *const u8,
    cert_len: usize,
    key_ptr: *const u8,
    key_len: usize,
    alpn_protocols_ptr: *const u8,
    alpn_protocols_len: usize,
    peer_fingerprint_ptr: *const u8,
    peer_fingerprint_len: usize,
    option_flags: u32,
    _reserved: u32,
}

/// Exact ABI v5 layout retained for callers that allocated only 144 bytes.
#[repr(C)]
#[derive(Clone, Copy)]
struct ElephcTlsClientOptionsV5 {
    abi_version: u32,
    verification_flags: u32,
    peer_name_ptr: *const u8,
    peer_name_len: usize,
    cafile_ptr: *const u8,
    cafile_len: usize,
    capath_ptr: *const u8,
    capath_len: usize,
    cert_ptr: *const u8,
    cert_len: usize,
    key_ptr: *const u8,
    key_len: usize,
    alpn_protocols_ptr: *const u8,
    alpn_protocols_len: usize,
    peer_fingerprint_ptr: *const u8,
    peer_fingerprint_len: usize,
    option_flags: u32,
    _reserved: u32,
    passphrase_ptr: *const u8,
    passphrase_len: usize,
}

impl TlsOptions<'_> {
    /// Returns the secure built-in-root configuration used by legacy wrappers.
    fn secure_defaults() -> Self {
        Self {
            verification_flags: TLS_DEFAULT_VERIFY_FLAGS,
            cafile: None,
            capath: None,
            client_cert: None,
            client_key: None,
            passphrase: None,
            alpn_protocols: None,
            peer_fingerprint: None,
            ciphers: None,
            security_level: None,
            disable_compression: None,
            option_flags: 0,
        }
    }
}

/// Certificate verifier that keeps chain validation, peer-name validation, and
/// handshake-signature validation independent, matching PHP stream options.
#[derive(Debug)]
struct PolicyVerifier {
    roots: Arc<RootCertStore>,
    policy: VerificationPolicy,
    supported: WebPkiSupportedAlgorithms,
}

impl ServerCertVerifier for PolicyVerifier {
    /// Applies the selected chain and name policy without weakening TLS
    /// CertificateVerify signature checks.
    fn verify_server_cert(
        &self,
        end_entity: &CertificateDer<'_>,
        intermediates: &[CertificateDer<'_>],
        server_name: &ServerName<'_>,
        _ocsp: &[u8],
        now: UnixTime,
    ) -> Result<ServerCertVerified, RustlsError> {
        let cert = ParsedCertificate::try_from(end_entity)?;
        if self.policy.verify_peer {
            let chain_result = verify_server_cert_signed_by_trust_anchor(
                &cert,
                &self.roots,
                intermediates,
                now,
                self.supported.all,
            );
            if let Err(chain_error) = chain_result {
                if !self.policy.allow_self_signed
                    || verify_depth_zero_self_signed(
                        end_entity,
                        &cert,
                        now,
                        self.supported.all,
                    )
                    .is_err()
                {
                    return Err(chain_error);
                }
            }
        }
        if self.policy.verify_peer_name {
            verify_server_name(&cert, server_name)?;
        }
        Ok(ServerCertVerified::assertion())
    }

    /// Cryptographically validates every TLS 1.2 CertificateVerify signature,
    /// including when peer-chain and peer-name checks are disabled.
    fn verify_tls12_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, RustlsError> {
        verify_tls12_signature(message, cert, dss, &self.supported)
    }

    /// Cryptographically validates every TLS 1.3 CertificateVerify signature,
    /// including when peer-chain and peer-name checks are disabled.
    fn verify_tls13_signature(
        &self,
        message: &[u8],
        cert: &CertificateDer<'_>,
        dss: &DigitallySignedStruct,
    ) -> Result<HandshakeSignatureValid, RustlsError> {
        verify_tls13_signature(message, cert, dss, &self.supported)
    }

    /// Reports exactly the signature schemes implemented by the ring provider.
    fn supported_verify_schemes(&self) -> Vec<SignatureScheme> {
        self.supported.supported_schemes()
    }
}

/// Verifies that a depth-zero certificate is self-issued and signed by its own
/// key, while still enforcing validity, purpose, and supported algorithms.
///
/// rustls-webpki has no dedicated self-signed-leaf API. Building a temporary
/// one-certificate trust store makes the public path verifier require the
/// leaf's issuer to equal its subject and verify its signature with its own
/// SPKI. Unrelated extra certificates do not change that depth-zero property.
fn verify_depth_zero_self_signed(
    end_entity: &CertificateDer<'_>,
    cert: &ParsedCertificate<'_>,
    now: UnixTime,
    supported_algs: &[&dyn rustls::pki_types::SignatureVerificationAlgorithm],
) -> Result<(), RustlsError> {
    let mut self_anchor = RootCertStore::empty();
    self_anchor.add(end_entity.clone())?;
    verify_server_cert_signed_by_trust_anchor(cert, &self_anchor, &[], now, supported_algs)
}

/// Builds a rustls client configuration with chain and name verification
/// disabled while retaining TLS handshake-signature verification.
fn insecure_client_config() -> Arc<ClientConfig> {
    static CFG: OnceLock<Arc<ClientConfig>> = OnceLock::new();
    CFG.get_or_init(|| {
        let mut options = TlsOptions::secure_defaults();
        options.verification_flags = 0;
        policy_client_config(options).expect("built-in TLS configuration must be valid")
    })
    .clone()
}

/// Builds the platform-default trust store used for server authentication.
///
/// Windows follows php-src and reads the native certificate store so
/// administrator-installed enterprise roots work. Other targets retain the
/// deterministic Mozilla-derived bundle used by the existing bridge.
fn bundled_root_store() -> RootCertStore {
    let mut roots = RootCertStore::empty();
    #[cfg(windows)]
    {
        let native = rustls_native_certs::load_native_certs();
        roots.add_parsable_certificates(native.certs);
    }
    #[cfg(not(windows))]
    roots.extend(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
    roots
}

/// Adds every certificate from one PEM bundle, rejecting an unreadable,
/// malformed, or certificate-free explicit `cafile`.
fn add_cafile_roots(roots: &mut RootCertStore, cafile_path: &str) -> Option<()> {
    let pem = std::fs::read(cafile_path).ok()?;
    let mut reader: &[u8] = &pem;
    let before = roots.len();
    for cert in rustls_pemfile::certs(&mut reader) {
        roots.add(cert.ok()?).ok()?;
    }
    (roots.len() > before).then_some(())
}

/// Adds certificates from PEM files in one explicit `capath`, ignoring
/// unrelated directory entries but rejecting an unreadable or empty source.
fn add_capath_roots(roots: &mut RootCertStore, capath: &str) -> Option<()> {
    let before = roots.len();
    for entry in std::fs::read_dir(capath).ok()? {
        let path = match entry {
            Ok(entry) => entry.path(),
            Err(_) => continue,
        };
        if !path.is_file() {
            continue;
        }
        let pem = match std::fs::read(path) {
            Ok(pem) => pem,
            Err(_) => continue,
        };
        let mut reader: &[u8] = &pem;
        for cert in rustls_pemfile::certs(&mut reader).flatten() {
            let _ = roots.add(cert);
        }
    }
    (roots.len() > before).then_some(())
}

/// Builds the trust store for one option set. Explicit `cafile` and `capath`
/// sources are additive to each other; when neither is present the bundled
/// Mozilla roots are used.
fn configured_root_store(options: TlsOptions<'_>) -> Option<RootCertStore> {
    if options.cafile.is_none() && options.capath.is_none() {
        return Some(bundled_root_store());
    }
    let mut roots = RootCertStore::empty();
    if let Some(cafile) = options.cafile {
        add_cafile_roots(&mut roots, cafile)?;
    }
    if let Some(capath) = options.capath {
        add_capath_roots(&mut roots, capath)?;
    }
    Some(roots)
}

/// Parses PHP's comma-separated ALPN option into rustls protocol names.
fn parse_alpn_protocols(value: Option<&str>) -> Option<Vec<Vec<u8>>> {
    let Some(value) = value else {
        return Some(Vec::new());
    };
    if value.is_empty() {
        return None;
    }
    let protocols = value
        .split(',')
        .map(|protocol| {
            let bytes = protocol.as_bytes();
            (!bytes.is_empty() && bytes.len() <= 255).then(|| bytes.to_vec())
        })
        .collect::<Option<Vec<_>>>()?;
    (!protocols.is_empty()).then_some(protocols)
}

/// Loads an unencrypted PEM client certificate chain and private key.
fn load_client_credentials(
    cert_path: &str,
    key_path: &str,
    passphrase: Option<&str>,
) -> Option<(Vec<CertificateDer<'static>>, PrivateKeyDer<'static>)> {
    let cert_pem = std::fs::read(cert_path).ok()?;
    let mut cert_reader: &[u8] = &cert_pem;
    let certs: Vec<CertificateDer<'static>> = rustls_pemfile::certs(&mut cert_reader)
        .collect::<Result<_, _>>()
        .ok()?;
    if certs.is_empty() {
        return None;
    }
    let key_pem = std::fs::read(key_path).ok()?;
    let key = load_private_key(&key_pem, passphrase)?;
    Some((certs, key))
}

/// Loads an ordinary PEM key or decrypts an `ENCRYPTED PRIVATE KEY` PKCS#8
/// document without ever including the passphrase in an error or log value.
fn load_private_key(key_pem: &[u8], passphrase: Option<&str>) -> Option<PrivateKeyDer<'static>> {
    let mut key_reader: &[u8] = key_pem;
    if let Some(key) = rustls_pemfile::private_key(&mut key_reader).ok().flatten() {
        return Some(key);
    }
    let passphrase = passphrase?;
    if let Some(pem) = std::str::from_utf8(key_pem).ok() {
        let begin = "-----BEGIN ENCRYPTED PRIVATE KEY-----";
        let end = "-----END ENCRYPTED PRIVATE KEY-----";
        if let Some(start) = pem.find(begin) {
            if let Some(finish) = pem[start..].find(end).map(|offset| offset + start + end.len()) {
                if let Some((_, document)) =
                    pkcs8::SecretDocument::from_pem(&pem[start..finish]).ok()
                {
                    if let Ok(encrypted) = pkcs8::EncryptedPrivateKeyInfo::try_from(document.as_bytes())
                    {
                        if let Ok(decrypted) = encrypted.decrypt(passphrase.as_bytes()) {
                            return Some(PrivateKeyDer::Pkcs8(PrivatePkcs8KeyDer::from(
                                decrypted.as_bytes().to_vec(),
                            )));
                        }
                    }
                }
            }
        }
    }
    load_legacy_encrypted_private_key(key_pem, passphrase)
}

/// Decrypts the legacy OpenSSL PEM envelope used by PHP's older RSA/EC key
/// fixtures (`Proc-Type: 4,ENCRYPTED` plus `DEK-Info`). This format predates
/// PKCS#8 and uses EVP_BytesToKey (MD5) followed by CBC/PKCS#7. The envelope
/// is decoded locally with RustCrypto only; no OpenSSL/system fallback exists.
fn load_legacy_encrypted_private_key(
    key_pem: &[u8],
    passphrase: &str,
) -> Option<PrivateKeyDer<'static>> {
    let pem = std::str::from_utf8(key_pem).ok()?;
    let rsa_begin = "-----BEGIN RSA PRIVATE KEY-----";
    let ec_begin = "-----BEGIN EC PRIVATE KEY-----";
    let (start, kind, begin_marker, end_marker) = match (pem.find(rsa_begin), pem.find(ec_begin)) {
        (Some(rsa), Some(ec)) if rsa <= ec => (
            rsa,
            LegacyPrivateKeyKind::Rsa,
            rsa_begin,
            "-----END RSA PRIVATE KEY-----",
        ),
        (Some(_), Some(ec)) => (
            ec,
            LegacyPrivateKeyKind::Ec,
            ec_begin,
            "-----END EC PRIVATE KEY-----",
        ),
        (Some(rsa), None) => (
            rsa,
            LegacyPrivateKeyKind::Rsa,
            rsa_begin,
            "-----END RSA PRIVATE KEY-----",
        ),
        (None, Some(ec)) => (
            ec,
            LegacyPrivateKeyKind::Ec,
            ec_begin,
            "-----END EC PRIVATE KEY-----",
        ),
        (None, None) => return None,
    };
    let finish = pem[start..].find(end_marker)? + start + end_marker.len();
    let mut lines = pem[start..finish].lines();
    if lines.next()?.trim() != begin_marker {
        return None;
    }
    let mut encrypted = false;
    let mut algorithm = None;
    let mut iv_hex = None;
    let mut in_body = false;
    let mut body = String::new();
    for line in lines {
        let line = line.trim();
        if line == end_marker {
            break;
        }
        if !in_body {
            if line.is_empty() {
                in_body = true;
            } else if let Some(value) = line.strip_prefix("Proc-Type:") {
                encrypted = value.trim() == "4,ENCRYPTED";
            } else if let Some(value) = line.strip_prefix("DEK-Info:") {
                let (name, iv) = value.trim().split_once(',')?;
                algorithm = Some(name.trim().to_ascii_uppercase());
                iv_hex = Some(iv.trim().to_owned());
            } else {
                return None;
            }
        } else {
            body.push_str(line);
        }
    }
    if !in_body || !encrypted {
        return None;
    }
    let algorithm = algorithm?;
    let iv_hex = iv_hex?;
    let iv = decode_hex(&iv_hex)?;
    let mut ciphertext = Zeroizing::new(base64::engine::general_purpose::STANDARD
        .decode(body.as_bytes())
        .ok()?);
    let (key_len, iv_len) = legacy_cipher_sizes(&algorithm)?;
    if iv.len() != iv_len || ciphertext.is_empty() || ciphertext.len() % iv_len != 0 {
        return None;
    }
    let key = evp_bytes_to_key(passphrase.as_bytes(), &iv, key_len);
    let mut plaintext = decrypt_legacy_cbc(&algorithm, &key, &iv, &mut ciphertext)?;
    let der = std::mem::take(&mut *plaintext);
    Some(match kind {
        LegacyPrivateKeyKind::Rsa => PrivateKeyDer::Pkcs1(PrivatePkcs1KeyDer::from(der)),
        LegacyPrivateKeyKind::Ec => PrivateKeyDer::Sec1(PrivateSec1KeyDer::from(der)),
    })
}

#[derive(Clone, Copy)]
enum LegacyPrivateKeyKind {
    Rsa,
    Ec,
}

/// OpenSSL's EVP_BytesToKey derivation for legacy PEM. Only the first eight
/// IV bytes are the salt, exactly as OpenSSL specifies for this envelope.
fn evp_bytes_to_key(passphrase: &[u8], iv: &[u8], key_len: usize) -> Zeroizing<Vec<u8>> {
    let mut key = Zeroizing::new(Vec::with_capacity(key_len));
    let mut previous = [0u8; 16];
    let mut have_previous = false;
    while key.len() < key_len {
        let mut input = Vec::with_capacity(
            usize::from(have_previous) * previous.len() + passphrase.len() + 8,
        );
        if have_previous {
            input.extend_from_slice(&previous);
        }
        input.extend_from_slice(passphrase);
        input.extend_from_slice(&iv[..8]);
        let mut digest = Md5::new();
        digest.update(&input);
        input.zeroize();
        let mut output = digest.finalize();
        previous.copy_from_slice(&output);
        key.extend_from_slice(&output);
        output.zeroize();
        have_previous = true;
    }
    previous.zeroize();
    key.truncate(key_len);
    key
}

fn decode_hex(value: &str) -> Option<Zeroizing<Vec<u8>>> {
    if value.is_empty() || value.len() % 2 != 0 {
        return None;
    }
    let mut decoded = Zeroizing::new(Vec::with_capacity(value.len() / 2));
    let bytes = value.as_bytes();
    for pair in bytes.chunks_exact(2) {
        let high = (pair[0] as char).to_digit(16)? as u8;
        let low = (pair[1] as char).to_digit(16)? as u8;
        decoded.push((high << 4) | low);
    }
    Some(decoded)
}

fn legacy_cipher_sizes(algorithm: &str) -> Option<(usize, usize)> {
    Some(match algorithm {
        "AES-128-CBC" => (16, 16),
        "AES-192-CBC" => (24, 16),
        "AES-256-CBC" => (32, 16),
        "DES-CBC" => (8, 8),
        "DES-EDE-CBC" => (16, 8),
        "DES-EDE3-CBC" => (24, 8),
        _ => return None,
    })
}

fn decrypt_legacy_cbc(
    algorithm: &str,
    key: &[u8],
    iv: &[u8],
    ciphertext: &mut [u8],
) -> Option<Zeroizing<Vec<u8>>> {
    macro_rules! decrypt {
        ($cipher:ty) => {{
            let decryptor = cbc::Decryptor::<$cipher>::new_from_slices(key, iv).ok()?;
            let plaintext = decryptor.decrypt_padded_mut::<Pkcs7>(ciphertext).ok()?;
            Some(Zeroizing::new(plaintext.to_vec()))
        }};
    }
    match algorithm {
        "AES-128-CBC" => decrypt!(Aes128),
        "AES-192-CBC" => decrypt!(Aes192),
        "AES-256-CBC" => decrypt!(Aes256),
        "DES-CBC" => decrypt!(Des),
        "DES-EDE-CBC" => decrypt!(TdesEde2),
        "DES-EDE3-CBC" => decrypt!(TdesEde3),
        _ => None,
    }
}

/// Builds the rustls server configuration used by PHP's server-side crypto
/// method. The certificate and key come from `ssl.local_cert` and
/// `ssl.local_pk`; when the key is omitted PHP also accepts the certificate
/// path as the key source. PKCS#8 encrypted keys are decrypted with the
/// optional `ssl.passphrase` before rustls validates the identity.
fn server_config(options: TlsOptions<'_>, crypto_method: i64) -> Option<Arc<ServerConfig>> {
    let cert_path = options.client_cert?;
    let key_path = options.client_key.unwrap_or(cert_path);
    let (certs, key) = load_client_credentials(cert_path, key_path, options.passphrase)?;
    let provider = Arc::new(configured_crypto_provider(options)?);
    let mut config = ServerConfig::builder_with_provider(provider)
        .with_protocol_versions(protocol_versions_for_crypto_method(crypto_method)?).ok()?
        .with_no_client_auth()
        .with_single_cert(certs, key)
        .ok()?;
    config.alpn_protocols = parse_alpn_protocols(options.alpn_protocols)?;
    if options.option_flags & TLS_OPTION_NO_TICKET != 0 {
        config.send_tls13_tickets = 0;
    }
    Some(Arc::new(config))
}

/// Builds one rustls configuration from the complete combinable PHP TLS
/// option set, using a single policy verifier for every configuration.
fn policy_client_config(options: TlsOptions<'_>) -> Option<Arc<ClientConfig>> {
    policy_client_config_for_method(options, PHP_STREAM_CRYPTO_TLS_CLIENT)
}

/// Selects the rustls-supported protocol versions requested by a PHP crypto method.
fn protocol_versions_for_crypto_method(
    crypto_method: i64,
) -> Option<&'static [&'static SupportedProtocolVersion]> {
    if crypto_method < 0
        || crypto_method & !PHP_STREAM_CRYPTO_KNOWN_BITS != 0
    {
        return None;
    }
    match (
        crypto_method & PHP_STREAM_CRYPTO_TLS_1_2 != 0,
        crypto_method & PHP_STREAM_CRYPTO_TLS_1_3 != 0,
    ) {
        (true, true) => Some(TLS_1_2_AND_1_3),
        (true, false) => Some(TLS_1_2_ONLY),
        (false, true) => Some(TLS_1_3_ONLY),
        (false, false) => None,
    }
}

/// Maps the exact cipher-suite spellings that OpenSSL exposes in a PHP stream
/// context to the ring-backed rustls provider. OpenSSL's cipher-list language
/// (aliases, exclusions, ordering operators, and `@SECLEVEL=` directives) is
/// deliberately not interpreted: accepting one without reproducing its set
/// would silently weaken or broaden the requested policy.
fn cipher_suite_from_php_name(name: &str) -> Option<CipherSuite> {
    match name {
        "ECDHE-ECDSA-AES128-GCM-SHA256" => {
            Some(CipherSuite::TLS_ECDHE_ECDSA_WITH_AES_128_GCM_SHA256)
        }
        "ECDHE-ECDSA-AES256-GCM-SHA384" => {
            Some(CipherSuite::TLS_ECDHE_ECDSA_WITH_AES_256_GCM_SHA384)
        }
        "ECDHE-ECDSA-CHACHA20-POLY1305" => {
            Some(CipherSuite::TLS_ECDHE_ECDSA_WITH_CHACHA20_POLY1305_SHA256)
        }
        "ECDHE-RSA-AES128-GCM-SHA256" => {
            Some(CipherSuite::TLS_ECDHE_RSA_WITH_AES_128_GCM_SHA256)
        }
        "ECDHE-RSA-AES256-GCM-SHA384" => {
            Some(CipherSuite::TLS_ECDHE_RSA_WITH_AES_256_GCM_SHA384)
        }
        "ECDHE-RSA-CHACHA20-POLY1305" => {
            Some(CipherSuite::TLS_ECDHE_RSA_WITH_CHACHA20_POLY1305_SHA256)
        }
        _ => None,
    }
}

/// Returns whether OpenSSL recognizes the token as a TLS 1.3 suite name.
/// `SSL_CTX_set_cipher_list`, which is the API php-src uses for `ssl.ciphers`,
/// ignores these tokens whenever at least one TLS <= 1.2 suite remains.
fn is_tls13_cipher_name(name: &str) -> bool {
    matches!(
        name,
        "TLS_AES_128_GCM_SHA256"
            | "TLS_AES_256_GCM_SHA384"
            | "TLS_CHACHA20_POLY1305_SHA256"
    )
}

/// Builds the only crypto-provider policy that can faithfully represent the
/// requested PHP options. OpenSSL's security level changes certificate,
/// signature, key-size, protocol, and cipher policy; rustls does not expose a
/// level-equivalent contract, so every explicit value fails closed. TLS
/// compression is absent from rustls, so `disable_compression=true` is
/// naturally satisfied while an explicit `false` fails closed instead of
/// pretending compression is enabled.
fn configured_crypto_provider(options: TlsOptions<'_>) -> Option<CryptoProvider> {
    if options.security_level.is_some() || options.disable_compression == Some(false)
    {
        return None;
    }
    let mut provider = rustls::crypto::ring::default_provider();
    let Some(cipher_list) = options.ciphers else {
        return Some(provider);
    };
    let mut selected_tls12: Vec<rustls::SupportedCipherSuite> = Vec::new();
    for name in cipher_list.split(':').map(str::trim) {
        if is_tls13_cipher_name(name) {
            continue;
        }
        let suite = cipher_suite_from_php_name(name)?;
        let candidate = provider
            .cipher_suites
            .iter()
            .find(|candidate| candidate.suite() == suite)
            .copied()?;
        if !selected_tls12
            .iter()
            .any(|current| current.suite() == suite)
        {
            selected_tls12.push(candidate);
        }
    }
    if selected_tls12.is_empty() {
        return None;
    }
    // PHP's `SSL_CTX_set_cipher_list` leaves TLS 1.3 suites untouched. Keep
    // rustls's default TLS 1.3 order and replace only the TLS 1.2 tail.
    provider.cipher_suites.retain(|suite| suite.tls13().is_some());
    provider.cipher_suites.extend(selected_tls12);
    Some(provider)
}

/// Builds a policy configuration restricted to the requested PHP TLS client versions.
fn policy_client_config_for_method(
    options: TlsOptions<'_>,
    crypto_method: i64,
) -> Option<Arc<ClientConfig>> {
    if crypto_method & PHP_STREAM_CRYPTO_IS_CLIENT == 0 {
        return None;
    }
    let policy = VerificationPolicy::from_flags(options.verification_flags)?;
    let protocol_versions = protocol_versions_for_crypto_method(crypto_method)?;
    // php-src only loads cafile/capath while verify_peer is enabled. A caller
    // performing name-only or fully relaxed verification must not fail because
    // an otherwise unused CA path is unreadable.
    let roots = Arc::new(if policy.verify_peer {
        configured_root_store(options)?
    } else {
        RootCertStore::empty()
    });
    let provider = Arc::new(configured_crypto_provider(options)?);
    let supported = provider.signature_verification_algorithms;
    let verifier = Arc::new(PolicyVerifier {
        roots,
        policy,
        supported,
    });
    let wants_client_auth = ClientConfig::builder_with_provider(provider)
        .with_protocol_versions(protocol_versions)
        .ok()?
        .dangerous()
        .with_custom_certificate_verifier(verifier);

    let mut config = match options.client_cert {
        None => Arc::new(wants_client_auth.with_no_client_auth()),
        Some(cert_path) => {
            // php-src falls back to local_cert as the private-key source when
            // local_pk is absent; local_pk alone does not enable client auth.
            let key_path = options.client_key.unwrap_or(cert_path);
            let (certs, key) = load_client_credentials(cert_path, key_path, options.passphrase)?;
            Arc::new(wants_client_auth
                .with_client_auth_cert(certs, key)
                .ok()?)
        }
    };
    let config_mut = Arc::make_mut(&mut config);
    config_mut.alpn_protocols = parse_alpn_protocols(options.alpn_protocols)?;
    config_mut.enable_sni = options.option_flags & TLS_OPTION_SNI_DISABLED == 0;
    if options.option_flags & TLS_OPTION_NO_TICKET != 0 {
        // rustls exposes no OpenSSL-style ClientConfig::enable_tickets field;
        // disabling the client resumption/ticket cache is its equivalent.
        config_mut.resumption = rustls::client::Resumption::disabled();
    }
    Some(config)
}

/// Returns the lazily initialized default rustls client configuration.
fn shared_client_config() -> Arc<ClientConfig> {
    static CFG: OnceLock<Arc<ClientConfig>> = OnceLock::new();
    CFG.get_or_init(|| {
        policy_client_config(TlsOptions::secure_defaults())
            .expect("built-in TLS configuration must be valid")
    })
    .clone()
}

/// Historical secure-default connect wrapper retained for ABI compatibility.
/// New runtime paths use `elephc_tls_connect_with_options`.
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

/// Historical no-chain/no-name connect wrapper retained for ABI compatibility.
/// TLS 1.2/1.3 CertificateVerify signatures remain cryptographically checked;
/// the uniform runtime expresses this policy with verification flags `0`.
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

/// Opens a TCP/TLS session using one uniform, combinable PHP option surface.
///
/// `peer_name` overrides the connection host for SNI and optional name
/// verification. `cafile` and `capath` are additive trust sources. `local_cert`
/// and `local_pk` enable client authentication; when `local_pk` is absent,
/// php-src semantics load the private key from `local_cert`.
///
/// # Safety
///
/// `options_ptr` must point to a readable `ElephcTlsClientOptions`. Every
/// non-null option pointer must reference its paired byte length for this call.
/// Text inputs must be UTF-8.
#[no_mangle]
pub unsafe extern "C" fn elephc_tls_connect_with_options(
    host_ptr: *const u8,
    host_len: usize,
    port: u16,
    options_ptr: *const ElephcTlsClientOptions,
) -> i64 {
    let Some((options, peer_name)) = tls_options_from_abi(options_ptr) else {
        return -1;
    };
    let Some(config) = policy_client_config(options) else {
        return -1;
    };
    tls_connect_inner_named(
        host_ptr,
        host_len,
        port,
        config,
        peer_name,
        options.peer_fingerprint,
    )
}

/// Builds a `ClientConfig` whose trust anchors come from the PEM bundle at
/// `cafile_path` instead of the built-in webpki-roots. Returns `None` if the
/// path is unreadable, contains no certificates, or any certificate is
/// malformed — the caller then fails the connect, matching PHP's behavior when
/// `ssl.cafile` cannot be loaded. Not cached: cafile connects are rare.
fn cafile_client_config(cafile_path: &str) -> Option<Arc<ClientConfig>> {
    let mut options = TlsOptions::secure_defaults();
    options.cafile = Some(cafile_path);
    policy_client_config(options)
}

/// Historical cafile-only connect wrapper retained for ABI compatibility.
/// New runtime paths combine cafile with other options through the uniform
/// option block. Returns `-1` for an unreadable or empty bundle.
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
    let mut options = TlsOptions::secure_defaults();
    options.capath = Some(capath);
    policy_client_config(options)
}

/// Historical capath-only connect wrapper retained for ABI compatibility.
/// New runtime paths combine capath with other options through the uniform
/// option block. Returns `-1` for an unreadable or empty directory.
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

/// Historical peer-name-only connect wrapper retained for ABI compatibility.
/// New runtime paths carry the SNI/name override in the uniform option block.
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
    tls_connect_inner_named(
        host_ptr,
        host_len,
        port,
        shared_client_config(),
        Some(peer_name),
        None,
    )
}

/// Builds a `ClientConfig` that presents a client certificate for mutual TLS:
/// the PEM certificate chain at `cert_path` and the PEM private key at
/// `key_path` (the `ssl.local_cert` / `ssl.local_pk` stream-context options).
/// The server is still authenticated against the built-in webpki-roots.
/// Returns `None` when either file is unreadable, the certificate chain is
/// empty, the private key cannot be parsed, or rustls rejects the key — the
/// caller then fails the connect, matching PHP's behavior when `ssl.local_cert`
/// cannot be loaded. PKCS#8 `ENCRYPTED PRIVATE KEY` files are decrypted with
/// the passphrase from `ssl.passphrase` without exposing it in diagnostics.
/// Not cached: client-cert connects are rare and each may use a distinct key.
fn client_cert_config(cert_path: &str, key_path: &str) -> Option<Arc<ClientConfig>> {
    let mut options = TlsOptions::secure_defaults();
    options.client_cert = Some(cert_path);
    options.client_key = Some(key_path);
    policy_client_config(options)
}

/// Historical explicit-client-key connect wrapper retained for ABI
/// compatibility. New runtime paths combine client identity and trust/policy
/// options through `elephc_tls_connect_with_options`.
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

/// Duplicates the caller's live TCP socket `fd` into a `TcpStream` that owns
/// an independent reference, so the caller's original fd remains valid for
/// its own I/O/close while the returned stream is used exclusively for TLS
/// framing. Returns `None` if the descriptor is not an IPv4/IPv6 stream
/// socket or if duplication fails. Shared by `elephc_tls_attach_fd` and
/// `elephc_tls_attach_fd_client_cert`.
///
/// # Safety
///
/// `fd` must refer to a connected TCP socket owned by the caller.
#[cfg(unix)]
unsafe fn dup_as_tcp_stream(fd: i64) -> Option<TcpStream> {
    let fd = i32::try_from(fd).ok()?;
    // STARTTLS can be requested on any PHP stream (including php://memory and
    // Unix-domain sockets), so validate the type and family before constructing
    // Rust's TCP-specific owning wrapper.
    let mut socket_type: libc::c_int = 0;
    let mut socket_type_len = std::mem::size_of::<libc::c_int>() as libc::socklen_t;
    if libc::getsockopt(
        fd,
        libc::SOL_SOCKET,
        libc::SO_TYPE,
        &mut socket_type as *mut libc::c_int as *mut libc::c_void,
        &mut socket_type_len,
    ) != 0
        || socket_type != libc::SOCK_STREAM
    {
        return None;
    }
    let mut socket_address: libc::sockaddr_storage = std::mem::zeroed();
    let mut socket_address_len =
        std::mem::size_of::<libc::sockaddr_storage>() as libc::socklen_t;
    if libc::getsockname(
        fd,
        &mut socket_address as *mut libc::sockaddr_storage as *mut libc::sockaddr,
        &mut socket_address_len,
    ) != 0
        || !matches!(
            socket_address.ss_family as libc::c_int,
            libc::AF_INET | libc::AF_INET6
        )
    {
        return None;
    }
    let dup_fd = libc::dup(fd);
    if dup_fd < 0 {
        return None;
    }
    Some(TcpStream::from_raw_fd(dup_fd))
}

/// Windows counterpart of the Unix `dup_as_tcp_stream` above. The incoming
/// value is a raw 64-bit Winsock `SOCKET`, not a CRT file descriptor. A
/// `ManuallyDrop<TcpStream>` provides a temporary borrowed view solely so
/// `try_clone()` can ask the standard library to duplicate the socket. The
/// borrowed view never closes the caller's socket; the returned clone is the
/// independently owned socket used and eventually closed by the TLS session.
/// Winsock validates the socket type and IP family before Rust constructs the
/// TCP-specific borrowed view. Non-socket, stale, and datagram handles return
/// `None`.
///
/// # Safety
///
/// `fd` must be a live Winsock `SOCKET` represented without truncation in an
/// `i64`. Ownership remains with the caller.
#[cfg(windows)]
unsafe fn dup_as_tcp_stream(fd: i64) -> Option<TcpStream> {
    if fd < 0 {
        return None;
    }
    let socket = fd as RawSocket;
    let winsock_socket = socket as libc::SOCKET;
    let mut socket_type: libc::c_int = 0;
    let mut socket_type_len = std::mem::size_of::<libc::c_int>() as libc::c_int;
    if libc::getsockopt(
        winsock_socket,
        WINDOWS_SOL_SOCKET,
        WINDOWS_SO_TYPE,
        &mut socket_type as *mut libc::c_int as *mut libc::c_char,
        &mut socket_type_len,
    ) != 0
        || socket_type != WINDOWS_SOCK_STREAM
    {
        return None;
    }

    // Winsock's SOCKADDR_STORAGE is 128 bytes and aligned to 64 bits. Use an
    // equivalently sized/aligned buffer so both IPv4 and IPv6 addresses fit.
    let mut socket_address = [0_u64; 16];
    let mut socket_address_len = std::mem::size_of_val(&socket_address) as libc::c_int;
    if libc::getsockname(
        winsock_socket,
        socket_address.as_mut_ptr().cast::<libc::sockaddr>(),
        &mut socket_address_len,
    ) != 0
    {
        return None;
    }
    let family = *(socket_address.as_ptr().cast::<u16>());
    if !matches!(family, WINDOWS_AF_INET | WINDOWS_AF_INET6) {
        return None;
    }

    let borrowed = ManuallyDrop::new(TcpStream::from_raw_socket(socket));
    borrowed.try_clone().ok()
}

/// Attaches TLS to an existing TCP socket using the same combinable option tail
/// as `elephc_tls_connect_with_options`.
///
/// # Safety
///
/// `fd` must refer to a live caller-owned TCP socket. `options_ptr` must point
/// to a readable option block whose non-null spans remain valid for this call.
/// When `crypto_method_present` is non-zero, `crypto_method` must be a supported
/// client or server method mask containing TLS 1.2 and/or TLS 1.3. Server
/// methods require `local_cert` and `local_pk` in the option block. A positive
/// `session_handle` may identify a live source client TLS stream whose context
/// and resumption cache should be reused; server methods never resume it.
#[no_mangle]
pub unsafe extern "C" fn elephc_tls_attach_fd_with_options(
    fd: i64,
    options_ptr: *const ElephcTlsClientOptions,
    crypto_method: i64,
    crypto_method_present: i64,
    session_handle: i64,
) -> i64 {
    if crypto_method_present == 0 {
        return -1;
    }
    let Some((options, peer_name)) = tls_options_from_abi(options_ptr) else {
        return -1;
    };
    if crypto_method & PHP_STREAM_CRYPTO_IS_CLIENT == 0 {
        if protocol_versions_for_crypto_method(crypto_method).is_none() {
            return -1;
        }
        let Some(config) = server_config(options, crypto_method) else {
            return -1;
        };
        let Some(sock) = dup_as_tcp_stream(fd) else {
            return -1;
        };
        let Ok(conn) = ServerConnection::new(config) else {
            return -1;
        };
        return insert_server_handle(sock, conn, options.peer_fingerprint);
    }
    let Some(policy) = VerificationPolicy::from_flags(options.verification_flags) else {
        return -1;
    };
    if peer_name.is_none() && policy.verify_peer_name {
        return -1;
    }
    if protocol_versions_for_crypto_method(crypto_method).is_none() {
        return -1;
    }
    let mut config = match session_client_config(session_handle) {
        Some(config) => config,
        None => {
            let Some(config) = policy_client_config_for_method(options, crypto_method) else {
                return -1;
            };
            config
        }
    };
    let peer_name = match peer_name {
        Some(peer_name) => peer_name,
        None => {
            Arc::make_mut(&mut config).enable_sni = false;
            "localhost"
        }
    };
    tls_attach_fd_inner(fd, peer_name, config, options.peer_fingerprint)
}

/// Historical explicit-client-key attach wrapper retained for ABI
/// compatibility. New runtime paths pass the complete option block through
/// `elephc_tls_attach_fd_with_options`.
///
/// # Safety
///
/// `fd` must refer to a connected TCP socket owned by the caller.
/// `peer_name_ptr`/`cert_ptr`/`key_ptr` must point to their respective lengths
/// of valid UTF-8 bytes for the duration of this call.
#[no_mangle]
pub unsafe extern "C" fn elephc_tls_attach_fd_client_cert(
    fd: i64,
    peer_name_ptr: *const u8,
    peer_name_len: usize,
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
    let Some(Some(peer_name)) = optional_utf8(peer_name_ptr, peer_name_len) else {
        return -1;
    };
    tls_attach_fd_inner(fd, peer_name, config, None)
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

/// Decodes one optional UTF-8 C-ABI byte span. Zero length means absent; a
/// nonzero length requires a non-null pointer and valid UTF-8.
///
/// # Safety
///
/// A non-null `ptr` must reference `len` readable bytes for this call.
unsafe fn optional_utf8<'a>(ptr: *const u8, len: usize) -> Option<Option<&'a str>> {
    if len == 0 {
        return Some(None);
    }
    if ptr.is_null() {
        return None;
    }
    std::str::from_utf8(std::slice::from_raw_parts(ptr, len))
        .ok()
        .map(Some)
}

/// Decodes the option tail shared by `connect_with_options` and
/// `attach_fd_with_options`.
///
/// # Safety
///
/// Every non-null pointer must reference its paired length for this call.
unsafe fn tls_options_from_raw<'a>(
    verification_flags: u32,
    cafile_ptr: *const u8,
    cafile_len: usize,
    capath_ptr: *const u8,
    capath_len: usize,
    cert_ptr: *const u8,
    cert_len: usize,
    key_ptr: *const u8,
    key_len: usize,
    passphrase_ptr: *const u8,
    passphrase_len: usize,
    alpn_protocols_ptr: *const u8,
    alpn_protocols_len: usize,
    peer_fingerprint_ptr: *const u8,
    peer_fingerprint_len: usize,
    ciphers_ptr: *const u8,
    ciphers_len: usize,
    security_level: i64,
    security_level_present: u32,
    disable_compression: u32,
    disable_compression_present: u32,
    option_flags: u32,
) -> Option<TlsOptions<'a>> {
    VerificationPolicy::from_flags(verification_flags)?;
    let client_cert = optional_utf8(cert_ptr, cert_len)?;
    let client_key = if client_cert.is_some() {
        optional_utf8(key_ptr, key_len)?
    } else {
        None
    };
    Some(TlsOptions {
        verification_flags,
        cafile: optional_utf8(cafile_ptr, cafile_len)?,
        capath: optional_utf8(capath_ptr, capath_len)?,
        client_cert,
        client_key,
        passphrase: optional_utf8(passphrase_ptr, passphrase_len)?,
        alpn_protocols: optional_utf8(alpn_protocols_ptr, alpn_protocols_len)?,
        peer_fingerprint: optional_utf8(peer_fingerprint_ptr, peer_fingerprint_len)?,
        ciphers: optional_utf8(ciphers_ptr, ciphers_len)?,
        security_level: (security_level_present != 0).then_some(security_level),
        disable_compression: (disable_compression_present != 0)
            .then_some(disable_compression != 0),
        option_flags,
    })
}

/// Decodes and version-checks the fixed-layout C option block.
///
/// # Safety
///
/// `options_ptr` and every non-null span it contains must remain readable for
/// the duration of this call.
unsafe fn tls_options_from_abi<'a>(
    options_ptr: *const ElephcTlsClientOptions,
) -> Option<(TlsOptions<'a>, Option<&'a str>)> {
    if options_ptr.is_null() {
        return None;
    }
    let version = std::ptr::read_unaligned(options_ptr.cast::<u32>());
    if matches!(version, TLS_CLIENT_OPTIONS_ABI_VERSION_1 | TLS_CLIENT_OPTIONS_ABI_VERSION_3) {
        let raw = &*options_ptr.cast::<ElephcTlsClientOptionsV3>();
        let peer_name = optional_utf8(raw.peer_name_ptr, raw.peer_name_len)?;
        let options = tls_options_from_raw(
            raw.verification_flags,
            raw.cafile_ptr,
            raw.cafile_len,
            raw.capath_ptr,
            raw.capath_len,
            raw.cert_ptr,
            raw.cert_len,
            raw.key_ptr,
            raw.key_len,
            std::ptr::null(),
            0,
            std::ptr::null(),
            0,
            std::ptr::null(),
            0,
            std::ptr::null(),
            0,
            0,
            0,
            0,
            0,
            0,
        )?;
        return Some((options, peer_name));
    }
    if version == TLS_CLIENT_OPTIONS_ABI_VERSION_4 {
        let raw = &*options_ptr.cast::<ElephcTlsClientOptionsV4>();
        let peer_name = optional_utf8(raw.peer_name_ptr, raw.peer_name_len)?;
        let options = tls_options_from_raw(
            raw.verification_flags,
            raw.cafile_ptr,
            raw.cafile_len,
            raw.capath_ptr,
            raw.capath_len,
            raw.cert_ptr,
            raw.cert_len,
            raw.key_ptr,
            raw.key_len,
            std::ptr::null(),
            0,
            raw.alpn_protocols_ptr,
            raw.alpn_protocols_len,
            raw.peer_fingerprint_ptr,
            raw.peer_fingerprint_len,
            std::ptr::null(),
            0,
            0,
            0,
            0,
            0,
            raw.option_flags,
        )?;
        return Some((options, peer_name));
    }
    if version == TLS_CLIENT_OPTIONS_ABI_VERSION_5 {
        let raw = &*options_ptr.cast::<ElephcTlsClientOptionsV5>();
        let peer_name = optional_utf8(raw.peer_name_ptr, raw.peer_name_len)?;
        let options = tls_options_from_raw(
            raw.verification_flags,
            raw.cafile_ptr,
            raw.cafile_len,
            raw.capath_ptr,
            raw.capath_len,
            raw.cert_ptr,
            raw.cert_len,
            raw.key_ptr,
            raw.key_len,
            raw.passphrase_ptr,
            raw.passphrase_len,
            raw.alpn_protocols_ptr,
            raw.alpn_protocols_len,
            raw.peer_fingerprint_ptr,
            raw.peer_fingerprint_len,
            std::ptr::null(),
            0,
            0,
            0,
            0,
            0,
            raw.option_flags,
        )?;
        return Some((options, peer_name));
    }
    if version != TLS_CLIENT_OPTIONS_ABI_VERSION {
        return None;
    }
    let raw = &*options_ptr;
    let peer_name = optional_utf8(raw.peer_name_ptr, raw.peer_name_len)?;
    let options = tls_options_from_raw(
        raw.verification_flags,
        raw.cafile_ptr,
        raw.cafile_len,
        raw.capath_ptr,
        raw.capath_len,
        raw.cert_ptr,
        raw.cert_len,
        raw.key_ptr,
        raw.key_len,
        if version == TLS_CLIENT_OPTIONS_ABI_VERSION {
            raw.passphrase_ptr
        } else {
            std::ptr::null()
        },
        if version == TLS_CLIENT_OPTIONS_ABI_VERSION {
            raw.passphrase_len
        } else {
            0
        },
        raw.alpn_protocols_ptr,
        raw.alpn_protocols_len,
        raw.peer_fingerprint_ptr,
        raw.peer_fingerprint_len,
        raw.ciphers_ptr,
        raw.ciphers_len,
        raw.security_level,
        raw.security_level_present,
        raw.disable_compression,
        raw.disable_compression_present,
        raw.option_flags,
    )?;
    Some((options, peer_name))
}

/// Creates a TCP connection, completes the TLS handshake, and stores the session handle.
unsafe fn tls_connect_inner(
    host_ptr: *const u8,
    host_len: usize,
    port: u16,
    config: Arc<ClientConfig>,
) -> i64 {
    tls_connect_inner_named(host_ptr, host_len, port, config, None, None)
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
    peer_fingerprint: Option<&str>,
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
    let conn = match ClientConnection::new(config.clone(), server_name) {
        Ok(c) => c,
        Err(_) => return -1,
    };
    insert_handle(sock, Connection::Client(conn), config, peer_fingerprint)
}

/// Creates a TLS session over a duplicated caller-owned TCP socket.
///
/// # Safety
///
/// `fd` must refer to a connected TCP socket that remains owned by the caller.
unsafe fn tls_attach_fd_inner(
    fd: i64,
    peer_name: &str,
    config: Arc<ClientConfig>,
    peer_fingerprint: Option<&str>,
) -> i64 {
    if fd < 0 || peer_name.is_empty() {
        return -1;
    }
    let server_name: ServerName<'static> = match ServerName::try_from(peer_name.to_string()) {
        Ok(name) => name,
        Err(_) => return -1,
    };
    let Some(sock) = dup_as_tcp_stream(fd) else {
        return -1;
    };
    let conn = match ClientConnection::new(config.clone(), server_name) {
        Ok(conn) => conn,
        Err(_) => return -1,
    };
    insert_handle(sock, Connection::Client(conn), config, peer_fingerprint)
}

/// Read up to `max_len` decrypted bytes from the TLS session into `buf_ptr`.
/// Returns the byte count, `0` on EOF, `-1` on a terminal error / unknown
/// handle, `-2` when the operation would block, or `-3` when it timed out.
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
    let mut guard = handles().lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let Some(entry) = guard.get_mut(&handle_id) else {
        return TLS_IO_TERMINAL;
    };
    let buf = std::slice::from_raw_parts_mut(buf_ptr, max_len);
    let result = match &mut entry.conn {
        Connection::Client(conn) => Stream::new(conn, &mut entry.sock).read(buf),
        Connection::Server(conn) => Stream::new(conn, &mut entry.sock).read(buf),
    };
    match result {
        Ok(n) => n as isize,
        Err(error) => tls_read_error_result(&error),
    }
}

/// Encrypt and send `len` bytes from `buf_ptr` over the TLS session. Returns
/// the byte count written, `-1` on a terminal error / unknown handle, `-2`
/// when the operation would block, or `-3` when it timed out.
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
    let mut guard = handles().lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let Some(entry) = guard.get_mut(&handle_id) else {
        return TLS_IO_TERMINAL;
    };
    let buf = std::slice::from_raw_parts(buf_ptr, len);
    let result = match &mut entry.conn {
        Connection::Client(conn) => Stream::new(conn, &mut entry.sock).write(buf),
        Connection::Server(conn) => Stream::new(conn, &mut entry.sock).write(buf),
    };
    match result {
        Ok(n) => n as isize,
        Err(error) => tls_io_error_result(&error),
    }
}

/// Historical secure-default attach wrapper retained for ABI compatibility.
/// New runtime paths use `elephc_tls_attach_fd_with_options`.
///
/// The wrapper duplicates `fd` via `libc::dup` so it owns its own
/// reference for the rustls `TcpStream`. The caller's original `fd`
/// remains valid but must not be used for I/O while the TLS session is
/// live — encrypted reads/writes would race with raw reads on the same
/// socket. The elephc runtime routes subsequent fread/fwrite/fclose
/// through the TLS handle instead of the bare fd.
///
/// Returns a handle ID, or `-1` on socket-duplication / SNI failure. The
/// caller must then use `elephc_tls_handshake` to complete or advance the
/// negotiation.
///
/// # Safety
///
/// `fd` must refer to a connected TCP socket owned by the caller.
/// `peer_name_ptr` must point to `peer_name_len` valid UTF-8 bytes for
/// the duration of this call; the peer name is used both for SNI and
/// for certificate-name validation by rustls.
#[no_mangle]
pub unsafe extern "C" fn elephc_tls_attach_fd(
    fd: i64,
    peer_name_ptr: *const u8,
    peer_name_len: usize,
) -> i64 {
    let Some(Some(peer_name)) = optional_utf8(peer_name_ptr, peer_name_len) else {
        return -1;
    };
    tls_attach_fd_inner(fd, peer_name, shared_client_config(), None)
}

/// Progresses the TLS handshake attached to `handle_id`.
///
/// Returns `1` after rustls has authenticated the peer and completed the
/// handshake, `0` when a nonblocking socket would block before completion,
/// and `-1` for an unknown handle, poisoned table, or terminal I/O/TLS error.
/// A `0` result deliberately retains the session so the next PHP
/// `stream_socket_enable_crypto(..., true)` call can continue it.
#[no_mangle]
pub extern "C" fn elephc_tls_handshake(handle_id: i64) -> i32 {
    let mut guard = handles()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let Some(entry) = guard.get_mut(&handle_id) else {
        return -1;
    };
    if !entry.conn.is_handshaking() {
        return 1;
    }

    match entry.conn.complete_io(&mut entry.sock) {
        Ok(_) if entry.conn.is_handshaking() => 0,
        Ok(_) => {
            if !verify_entry_fingerprint(entry) {
                return -1;
            }
            entry.fingerprint_checked = true;
            1
        }
        Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => 0,
        Err(_) => -1,
    }
}

/// Send a TLS close_notify, drop the underlying socket, and remove the
/// session from the handle table.
#[no_mangle]
pub extern "C" fn elephc_tls_close(handle_id: i64) {
    let mut guard = handles().lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    if let Some(mut entry) = guard.remove(&handle_id) {
        entry.conn.send_close_notify();
        match &mut entry.conn {
            Connection::Client(conn) => {
                let _ = Stream::new(conn, &mut entry.sock).flush();
            }
            Connection::Server(conn) => {
                let _ = Stream::new(conn, &mut entry.sock).flush();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verifies an earlier panic while holding the handle-table lock does not
    /// turn every subsequent TLS ABI operation into a process abort.
    #[test]
    fn poisoned_handle_table_is_recovered_by_tls_operations() {
        const TEST_NAME: &str = "poisoned_handle_table_is_recovered_by_tls_operations";
        if std::env::var("ELEPHC_TLS_POISON_PROBE").as_deref() != Ok(TEST_NAME) {
            let output = std::process::Command::new(std::env::current_exe().unwrap())
                .args([TEST_NAME, "--nocapture", "--test-threads=1"])
                .env("ELEPHC_TLS_POISON_PROBE", TEST_NAME)
                .output()
                .expect("spawn isolated TLS poison probe");
            assert!(
                output.status.success(),
                "isolated TLS poison probe failed:\n{}",
                String::from_utf8_lossy(&output.stderr)
            );
            return;
        }

        let poisoned = std::panic::catch_unwind(|| {
            let _guard = handles().lock().unwrap();
            panic!("intentional TLS handle-table poison");
        });
        assert!(poisoned.is_err(), "the probe must poison the table first");

        let mut buf = [0u8; 8];
        let read = unsafe { elephc_tls_read(0xDEAD_BEEF, buf.as_mut_ptr(), buf.len()) };
        assert_eq!(read, -1, "recovered lookup must preserve the unknown-handle sentinel");

        let listener = std::net::TcpListener::bind(("127.0.0.1", 0))
            .expect("bind isolated TLS connect probe");
        let port = listener.local_addr().expect("read TLS probe address").port();
        let accept = std::thread::spawn(move || {
            let _ = listener.accept().expect("accept isolated TLS connect probe");
        });
        let host = b"127.0.0.1";
        let handle = unsafe {
            tls_connect_inner(
                host.as_ptr(),
                host.len(),
                port,
                shared_client_config(),
            )
        };
        assert!(
            handle >= 0,
            "plain TLS connect must recover the poisoned handle table before insertion"
        );
        elephc_tls_close(handle);
        accept.join().expect("join isolated TLS connect probe");
    }

    /// Verifies that the exported TLS ABI version exposes the append-only v6
    /// option block while the decoder still accepts older prefixes.
    #[test]
    fn version_is_v6() {
        assert_eq!(elephc_tls_version(), 6);
    }

    /// Verifies the C ABI bits independently select chain, name, and
    /// self-signed policy and reject unrecognised bits.
    #[test]
    fn verification_policy_decodes_independent_bits() {
        assert_eq!(
            VerificationPolicy::from_flags(TLS_DEFAULT_VERIFY_FLAGS),
            Some(VerificationPolicy {
                verify_peer: true,
                verify_peer_name: true,
                allow_self_signed: false,
            }),
        );
        assert_eq!(
            VerificationPolicy::from_flags(TLS_VERIFY_PEER_NAME | TLS_ALLOW_SELF_SIGNED),
            Some(VerificationPolicy {
                verify_peer: false,
                verify_peer_name: true,
                allow_self_signed: true,
            }),
        );
        assert!(VerificationPolicy::from_flags(8).is_none());
    }

    /// Verifies the option block layout consumed by x86_64/aarch64 runtime adapters.
    #[test]
    fn client_options_c_layout_matches_runtime_contract() {
        assert_eq!(std::mem::size_of::<ElephcTlsClientOptions>(), 184);
        assert_eq!(
            std::mem::offset_of!(ElephcTlsClientOptions, abi_version),
            0
        );
        assert_eq!(
            std::mem::offset_of!(ElephcTlsClientOptions, verification_flags),
            4
        );
        assert_eq!(
            std::mem::offset_of!(ElephcTlsClientOptions, peer_name_ptr),
            8
        );
        assert_eq!(
            std::mem::offset_of!(ElephcTlsClientOptions, peer_name_len),
            16
        );
        assert_eq!(
            std::mem::offset_of!(ElephcTlsClientOptions, cafile_ptr),
            24
        );
        assert_eq!(
            std::mem::offset_of!(ElephcTlsClientOptions, cafile_len),
            32
        );
        assert_eq!(
            std::mem::offset_of!(ElephcTlsClientOptions, capath_ptr),
            40
        );
        assert_eq!(
            std::mem::offset_of!(ElephcTlsClientOptions, capath_len),
            48
        );
        assert_eq!(
            std::mem::offset_of!(ElephcTlsClientOptions, cert_ptr),
            56
        );
        assert_eq!(
            std::mem::offset_of!(ElephcTlsClientOptions, cert_len),
            64
        );
        assert_eq!(
            std::mem::offset_of!(ElephcTlsClientOptions, key_ptr),
            72
        );
        assert_eq!(
            std::mem::offset_of!(ElephcTlsClientOptions, key_len),
            80
        );
        assert_eq!(
            std::mem::offset_of!(ElephcTlsClientOptions, alpn_protocols_ptr),
            88
        );
        assert_eq!(
            std::mem::offset_of!(ElephcTlsClientOptions, peer_fingerprint_ptr),
            104
        );
        assert_eq!(
            std::mem::offset_of!(ElephcTlsClientOptions, option_flags),
            120
        );
        assert_eq!(
            std::mem::offset_of!(ElephcTlsClientOptions, passphrase_ptr),
            128
        );
        assert_eq!(
            std::mem::offset_of!(ElephcTlsClientOptions, passphrase_len),
            136
        );
        assert_eq!(
            std::mem::offset_of!(ElephcTlsClientOptions, ciphers_ptr),
            144
        );
        assert_eq!(
            std::mem::offset_of!(ElephcTlsClientOptions, ciphers_len),
            152
        );
        assert_eq!(
            std::mem::offset_of!(ElephcTlsClientOptions, security_level),
            160
        );
        assert_eq!(
            std::mem::offset_of!(ElephcTlsClientOptions, security_level_present),
            168
        );
        assert_eq!(
            std::mem::offset_of!(ElephcTlsClientOptions, disable_compression),
            172
        );
        assert_eq!(
            std::mem::offset_of!(ElephcTlsClientOptions, disable_compression_present),
            176
        );
    }

    /// Verifies the uniform connect entry rejects unknown policy bits before
    /// touching the network or any optional pointer.
    #[test]
    fn connect_options_rejects_unknown_verification_bits() {
        let host = "127.0.0.1";
        let options = ElephcTlsClientOptions {
            abi_version: TLS_CLIENT_OPTIONS_ABI_VERSION,
            verification_flags: 8,
            peer_name_ptr: std::ptr::null(),
            peer_name_len: 0,
            cafile_ptr: std::ptr::null(),
            cafile_len: 0,
            capath_ptr: std::ptr::null(),
            capath_len: 0,
            cert_ptr: std::ptr::null(),
            cert_len: 0,
            key_ptr: std::ptr::null(),
            key_len: 0,
            passphrase_ptr: std::ptr::null(),
            passphrase_len: 0,
            alpn_protocols_ptr: std::ptr::null(),
            alpn_protocols_len: 0,
            peer_fingerprint_ptr: std::ptr::null(),
            peer_fingerprint_len: 0,
            ciphers_ptr: std::ptr::null(),
            ciphers_len: 0,
            security_level: 0,
            security_level_present: 0,
            disable_compression: 0,
            disable_compression_present: 0,
            option_flags: 0,
            _reserved: 0,
        };
        let id = unsafe {
            elephc_tls_connect_with_options(
                host.as_ptr(),
                host.len(),
                9,
                &options,
            )
        };
        assert_eq!(id, -1);
    }

    /// Verifies an unknown session cannot be mistaken for successful TLS progress.
    #[test]
    fn handshake_unknown_handle_returns_minus_one() {
        assert_eq!(elephc_tls_handshake(0xDEAD_BEEF), -1);
    }

    /// Verifies a nonblocking socket reports retryable handshake progress and
    /// keeps its rustls session installed for the next call.
    #[test]
    fn handshake_would_block_preserves_session() {
        #[cfg(unix)]
        use std::os::fd::AsRawFd;
        #[cfg(windows)]
        use std::os::windows::io::AsRawSocket;

        let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let address = listener.local_addr().unwrap();
        let client = TcpStream::connect(address).unwrap();
        let _server = listener.accept().unwrap().0;
        client.set_nonblocking(true).unwrap();
        let peer = "localhost";
        #[cfg(unix)]
        let id = unsafe { elephc_tls_attach_fd(client.as_raw_fd() as i64, peer.as_ptr(), peer.len()) };
        #[cfg(windows)]
        let id = unsafe {
            elephc_tls_attach_fd(client.as_raw_socket() as i64, peer.as_ptr(), peer.len())
        };

        assert!(id > 0);
        assert!(
            session_client_config(id).is_some(),
            "a live TLS handle must retain its reusable client context"
        );
        assert_eq!(elephc_tls_handshake(id), 0);
        assert!(handles().lock().unwrap().contains_key(&id));
        elephc_tls_close(id);
    }

    /// Verifies the platform-default trust policy contains usable anchors.
    #[test]
    fn bundled_trust_store_is_populated() {
        #[cfg(windows)]
        assert!(!bundled_root_store().is_empty());
        #[cfg(not(windows))]
        assert!(bundled_root_store().len() > 100);
    }

    /// Verifies PHP crypto-method masks select rustls-supported versions and
    /// reject obsolete or unknown method requests while accepting the server
    /// TLS 1.2/1.3 masks.
    #[test]
    fn crypto_method_selects_supported_client_protocol_versions() {
        assert!(protocol_versions_for_crypto_method(0).is_none());
        assert_eq!(
            protocol_versions_for_crypto_method(PHP_STREAM_CRYPTO_TLS_CLIENT)
                .unwrap()
                .len(),
            TLS_1_2_AND_1_3.len()
        );
        let tls12 = protocol_versions_for_crypto_method(33).unwrap();
        assert_eq!(tls12.len(), 1);
        assert!(std::ptr::eq(tls12[0], &rustls::version::TLS12));
        let tls13 = protocol_versions_for_crypto_method(65).unwrap();
        assert_eq!(tls13.len(), 1);
        assert!(std::ptr::eq(tls13[0], &rustls::version::TLS13));
        assert!(protocol_versions_for_crypto_method(32).is_some());
        assert!(protocol_versions_for_crypto_method(64).is_some());
        assert!(protocol_versions_for_crypto_method(3).is_none());
        assert!(protocol_versions_for_crypto_method(129).is_none());
    }

    /// Verifies v5 ALPN parsing and the SNI/ticket switches are applied to a
    /// rustls client configuration rather than merely transported.
    #[test]
    fn v5_client_options_apply_alpn_sni_and_ticket_policy() {
        let config = policy_client_config(TlsOptions {
            verification_flags: TLS_DEFAULT_VERIFY_FLAGS,
            cafile: None,
            capath: None,
            client_cert: None,
            client_key: None,
            passphrase: None,
            alpn_protocols: Some("h2,http/1.1"),
            peer_fingerprint: None,
            ciphers: None,
            security_level: None,
            disable_compression: None,
            option_flags: TLS_OPTION_SNI_DISABLED | TLS_OPTION_NO_TICKET,
        })
        .expect("valid v5 client options");
        assert_eq!(
            config.alpn_protocols,
            vec![b"h2".to_vec(), b"http/1.1".to_vec()]
        );
        assert!(!config.enable_sni);
    }

    /// Verifies the v6 cipher option accepts only exact rustls-backed suite
    /// names and preserves the caller's order in both client and server
    /// provider construction.
    #[test]
    fn v6_ciphers_filter_provider_and_reject_openssl_aliases() {
        let options = TlsOptions {
            ciphers: Some("ECDHE-RSA-AES128-GCM-SHA256:TLS_AES_256_GCM_SHA384"),
            ..TlsOptions::secure_defaults()
        };
        let provider = configured_crypto_provider(options).expect("exact suites are supported");
        assert!(provider.cipher_suites[..provider.cipher_suites.len() - 1]
            .iter()
            .all(|suite| suite.tls13().is_some()));
        assert_eq!(
            provider.cipher_suites.last().unwrap().suite(),
            CipherSuite::TLS_ECDHE_RSA_WITH_AES_128_GCM_SHA256
        );
        assert!(configured_crypto_provider(TlsOptions {
            ciphers: Some("TLS_AES_256_GCM_SHA384"),
            ..TlsOptions::secure_defaults()
        })
        .is_none());
        assert!(configured_crypto_provider(TlsOptions {
            ciphers: Some("ecdhe-rsa-aes128-gcm-sha256"),
            ..TlsOptions::secure_defaults()
        })
        .is_none());
        assert!(configured_crypto_provider(TlsOptions {
            ciphers: Some("HIGH:!aNULL"),
            ..TlsOptions::secure_defaults()
        })
        .is_none());
    }

    /// Verifies options that rustls cannot represent never silently broaden or
    /// weaken a TLS policy, for both the client and server construction paths.
    #[test]
    fn v6_security_and_compression_options_fail_closed_when_unrepresentable() {
        for options in [
            TlsOptions {
                security_level: Some(1),
                ..TlsOptions::secure_defaults()
            },
            TlsOptions {
                disable_compression: Some(false),
                ..TlsOptions::secure_defaults()
            },
        ] {
            assert!(policy_client_config(options).is_none());
            assert!(configured_crypto_provider(options).is_none());
        }
        assert!(configured_crypto_provider(TlsOptions {
            disable_compression: Some(true),
            ..TlsOptions::secure_defaults()
        })
        .is_some());
    }

    /// Verifies PHP's scalar MD5/SHA-1/SHA-256 fingerprint forms selected by
    /// their canonical hexadecimal lengths.
    #[test]
    fn peer_fingerprint_matches_md5_sha1_and_sha256_certificate_digests() {
        let certificate = parse_test_certificate(TEST_SELF_SIGNED_SERVER_CERT_PEM);
        let md5 = hex_digest(&Md5::digest(certificate.as_ref()));
        let sha1 = hex_digest(&Sha1::digest(certificate.as_ref()));
        let sha256 = hex_digest(&Sha256::digest(certificate.as_ref()));
        assert!(fingerprint_value_matches_certificate(
            certificate.as_ref(),
            &md5,
            None,
        ));
        assert!(fingerprint_value_matches_certificate(
            certificate.as_ref(),
            &sha1,
            None,
        ));
        assert!(fingerprint_value_matches_certificate(
            certificate.as_ref(),
            &sha256,
            Some("sha256"),
        ));
        assert!(!fingerprint_value_matches_certificate(
            certificate.as_ref(),
            &"0".repeat(64),
            None,
        ));
    }

    /// A PHP fingerprint is an additional check: it does not disable the
    /// independent chain/name policy flags used by the TLS verifier.
    #[test]
    fn peer_fingerprint_does_not_weaken_verification_policy() {
        let fingerprint = "0".repeat(32);
        let options = TlsOptions {
            verification_flags: TLS_DEFAULT_VERIFY_FLAGS,
            cafile: Some("/nonexistent/fingerprint-ca.pem"),
            capath: None,
            client_cert: None,
            client_key: None,
            passphrase: None,
            alpn_protocols: None,
            peer_fingerprint: Some(fingerprint.as_str()),
            ciphers: None,
            security_level: None,
            disable_compression: None,
            option_flags: 0,
        };
        assert!(policy_client_config(options).is_none());
    }

    /// Verifies malformed certificate/SNI names fail before socket adoption or
    /// network I/O, keeping the C ABI on its `-1` sentinel contract.
    #[test]
    fn attach_rejects_invalid_sni_name() {
        let invalid_name = "not a valid dns name";
        let id = unsafe {
            elephc_tls_attach_fd(3, invalid_name.as_ptr(), invalid_name.len())
        };
        assert_eq!(id, -1);
    }

    /// Verifies STARTTLS rejects a regular descriptor before Rust adopts it as a socket.
    #[cfg(unix)]
    #[test]
    fn attach_rejects_non_socket_descriptor() {
        use std::os::fd::AsRawFd;

        let file = std::fs::File::open("/dev/null").unwrap();
        assert!(unsafe { dup_as_tcp_stream(file.as_raw_fd() as i64) }.is_none());
    }

    /// Verifies STARTTLS rejects datagram sockets before Rust adopts them as TCP streams.
    #[cfg(unix)]
    #[test]
    fn attach_rejects_non_stream_socket_descriptor() {
        use std::os::fd::AsRawFd;

        let socket = std::os::unix::net::UnixDatagram::unbound().unwrap();
        assert!(unsafe { dup_as_tcp_stream(socket.as_raw_fd() as i64) }.is_none());
    }

    /// Verifies STARTTLS rejects non-IP stream sockets before TCP adoption.
    #[cfg(unix)]
    #[test]
    fn attach_rejects_unix_stream_descriptor() {
        use std::os::fd::AsRawFd;

        let (socket, _peer) = std::os::unix::net::UnixStream::pair().unwrap();
        assert!(unsafe { dup_as_tcp_stream(socket.as_raw_fd() as i64) }.is_none());
    }

    /// Verifies Windows STARTTLS duplicates a raw 64-bit Winsock `SOCKET` into
    /// independently owned Rust streams without consuming the original.
    #[cfg(windows)]
    #[test]
    fn windows_socket_adoption_preserves_original_ownership() {
        use std::os::windows::io::AsRawSocket;

        let listener = std::net::TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let address = listener.local_addr().unwrap();
        let server = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            let mut bytes = [0_u8; 2];
            stream.read_exact(&mut bytes).unwrap();
            bytes
        });
        let mut original = TcpStream::connect(address).unwrap();
        let raw_socket = original.as_raw_socket() as i64;
        let mut duplicate = unsafe { dup_as_tcp_stream(raw_socket) }
            .expect("a live Winsock socket must be clonable");
        original.write_all(b"a").unwrap();
        duplicate.write_all(b"b").unwrap();
        assert_eq!(server.join().unwrap(), *b"ab");
    }

    /// Verifies Windows STARTTLS rejects UDP before constructing a TCP-specific
    /// borrowed view over the caller-owned Winsock socket.
    #[cfg(windows)]
    #[test]
    fn windows_socket_adoption_rejects_udp() {
        use std::os::windows::io::AsRawSocket;

        let socket = std::net::UdpSocket::bind(("127.0.0.1", 0)).unwrap();
        assert!(unsafe { dup_as_tcp_stream(socket.as_raw_socket() as i64) }.is_none());
    }

    /// Verifies reads from an unknown TLS handle fail with -1.
    #[test]
    fn unknown_handle_read_returns_minus_one() {
        let mut buf = [0u8; 16];
        let n = unsafe { elephc_tls_read(0xDEAD_BEEF, buf.as_mut_ptr(), buf.len()) };
        assert_eq!(n, -1);
    }

    /// Verifies writes to an unknown TLS handle fail with -1.
    #[test]
    fn unknown_handle_write_returns_minus_one() {
        let buf = [0u8; 4];
        let n = unsafe { elephc_tls_write(0xDEAD_BEEF, buf.as_ptr(), buf.len()) };
        assert_eq!(n, -1);
    }

    /// Verifies TLS reads normalize unclean peer EOF while preserving the
    /// retryable and terminal result classes used by the runtime ABI.
    #[test]
    fn io_error_results_distinguish_would_block_and_timeout() {
        assert_eq!(
            tls_read_error_result(&std::io::Error::from(std::io::ErrorKind::UnexpectedEof)),
            0,
        );
        assert_eq!(
            tls_io_error_result(&std::io::Error::from(std::io::ErrorKind::UnexpectedEof)),
            TLS_IO_TERMINAL,
        );
        assert_eq!(
            tls_io_error_result(&std::io::Error::from(std::io::ErrorKind::WouldBlock)),
            TLS_IO_WOULD_BLOCK,
        );
        assert_eq!(
            tls_io_error_result(&std::io::Error::from(std::io::ErrorKind::TimedOut)),
            TLS_IO_TIMED_OUT,
        );
        assert_eq!(
            tls_io_error_result(&std::io::Error::from(std::io::ErrorKind::ConnectionReset)),
            TLS_IO_TERMINAL,
        );
    }

    /// Verifies closing an unknown TLS handle is harmless.
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

    /// A depth-zero self-signed server leaf with CA=false, serverAuth EKU, and
    /// `selfsigned.test` SAN, used to validate the narrow allow_self_signed path.
    const TEST_SELF_SIGNED_SERVER_CERT_PEM: &str = "\
-----BEGIN CERTIFICATE-----
MIIDVTCCAj2gAwIBAgIUdYzgZUSy+xkyLYYhX75quV1Muo0wDQYJKoZIhvcNAQEL
BQAwGjEYMBYGA1UEAwwPc2VsZnNpZ25lZC50ZXN0MB4XDTI2MDcyNDA5MTUxNFoX
DTM2MDcyMTA5MTUxNFowGjEYMBYGA1UEAwwPc2VsZnNpZ25lZC50ZXN0MIIBIjAN
BgkqhkiG9w0BAQEFAAOCAQ8AMIIBCgKCAQEAqDpJ3TCfmAyNh/slhOE+8ytnHzi+
w4hIan0j3OamDcbhnxLqvkMK9NmeQlZn8F0yoSTDlsz7hqmfrB3+J4DTPkC71zPk
7Ka6Jwy9Jt0ctUvFhKtLMy1FC9y5GKLBYFws7BTYCKeDvLdCbLEoiK5vV26EPvWi
dVMiPVJCanrKA6VBRAmzcupLlCXvBSp688ybXLTgeJWz8Nlxm7Md4AT7c7qJo7/n
2Y1KS/Q7y5lB6Ybk4ITq6uqtBsTgn51emKJvu81oBuP13A2NcNShOjt3wR/H07KB
cvZ0nFut7vMsS6ju3DQ+jv+ekRFguIGVwau+U+PxLp6RxvRZnOgCfWNs3QIDAQAB
o4GSMIGPMB0GA1UdDgQWBBSY+M5cJiLOEpgcnBdC9TMO4Fx3cTAfBgNVHSMEGDAW
gBSY+M5cJiLOEpgcnBdC9TMO4Fx3cTAMBgNVHRMBAf8EAjAAMA4GA1UdDwEB/wQE
AwIFoDATBgNVHSUEDDAKBggrBgEFBQcDATAaBgNVHREEEzARgg9zZWxmc2lnbmVk
LnRlc3QwDQYJKoZIhvcNAQELBQADggEBAB5/dtNVauwCRp1KSWjVijq4K8dQLqZF
d2etZP1Ka0OWNzHCakGV2pCGgcZ60jj8a5b95mR+mQf1LN+s5TnzMilP20eLdgW7
fMroazs1SnEOZ/ElSABfR/BTfsSp2fm8PWppavoj5wOHSfWYq56z4DZC4EBxcYPU
MTh6oDwwRdDwkOc69nhoehAckMg6pMr/Ci522CfDW99Tq3dyv0/h6a8ijfg0l7QE
ZBT0YvX/20gSTQagNJIZg+b6g+IyZvZuXun4CnqTb0ob4bOvXxEA/58HZQGkb9jP
YAK54Ac858ia4r2WTk4QiSo0kRAHs+cVxNvyeTgyVV4ycRwgypT7VWc=
-----END CERTIFICATE-----
";

    /// A leaf signed by a distinct test CA, presented without that CA, used to
    /// prove allow_self_signed cannot turn an arbitrary leaf into its own anchor.
    const TEST_CA_SIGNED_SERVER_CERT_PEM: &str = "\
-----BEGIN CERTIFICATE-----
MIIDAjCCAeqgAwIBAgIUZl/TmUB6I7YV7IFtYCsqcc/qb5kwDQYJKoZIhvcNAQEL
BQAwGTEXMBUGA1UEAwwOZWxlcGhjLXRlc3QtY2EwHhcNMjYwNzI0MDkxOTUwWhcN
MzYwNzIxMDkxOTUwWjAZMRcwFQYDVQQDDA5jYS1zaWduZWQudGVzdDCCASIwDQYJ
KoZIhvcNAQEBBQADggEPADCCAQoCggEBANsTLmidtKeWo6MC39rGo/g8rWpUIqzb
+26vLUDpRovlYHm2MK5eEPDXGrqWxxubH0GT0Ua4sV9812/V23hHBUzM0oBlYAme
HYkpYJub47KoZpvEG5cZq1uzBcphTZAX5vRrLCzxdYYLyBIsgfkhFQFN/I1kHuK8
hHHHgVqT2bQWyUFM7vJ/t5GGnM9gFcIObgg03thE27DU3Vv0AmZYJKgMKfZVfg3t
1r/R3tiisFvqj8cyJNDogFTWy1FjCXMuRlRJFVWDeswEAUuFQpLt9x4rnQKX2R7G
UdXEDBjYwokKPX+qznChwJdxLadzJt0CQrNeu5mSqiEaYY6jpQmQ168CAwEAAaNC
MEAwHQYDVR0OBBYEFHMU9kY9m8TpUR1OnlKQLvvh4TjFMB8GA1UdIwQYMBaAFACe
ZsNYtl/ZFHEY2Zz2AkKB94cJMA0GCSqGSIb3DQEBCwUAA4IBAQAHt9j7kWmjMBSs
v/BXDJTH4RKx3jZj44u6qRalm3zzjboq4uN26ZSBFRnL+f/Tmb7VJYgcqA28aRKW
vuT2FnE6jdhOyz9h2lvMc+BTltfGStQbIkwoso5pbw3JWnPlxQK1K3G/r8nDBcbm
U5jj/FhO1e3H+u00nJZWqGafEMbBH7atJ1uiOM4pjHmWqK5zwJ++MwEATH2bDvh9
28V1qKnb/9ufWkM1mV79rDZNRtY6YCu9p1pGCU/lDjH+cIcsAy5jmsedA3kYCZM7
TkL58PKfwrEu7uYhU6/iajFyD0Fnl/1vKszLSo/fUfwGdEPAqoXcU/OukI9B0r8W
hylNBvLT
-----END CERTIFICATE-----
";

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

    /// Parses the first certificate from one test PEM fixture.
    fn parse_test_certificate(pem: &'static str) -> CertificateDer<'static> {
        let mut reader = pem.as_bytes();
        let certificate = rustls_pemfile::certs(&mut reader)
            .next()
            .expect("test PEM must contain a certificate")
            .expect("test certificate DER must be valid");
        certificate
    }

    /// Constructs a verifier with deterministic ring algorithms for policy tests.
    fn test_policy_verifier(flags: u32, roots: RootCertStore) -> PolicyVerifier {
        let provider = rustls::crypto::ring::default_provider();
        PolicyVerifier {
            roots: Arc::new(roots),
            policy: VerificationPolicy::from_flags(flags).unwrap(),
            supported: provider.signature_verification_algorithms,
        }
    }

    /// Verifies allow_self_signed accepts only a cryptographically self-signed
    /// depth-zero server leaf and keeps peer-name validation independent.
    #[test]
    fn policy_verifier_limits_self_signed_fallback_to_depth_zero() {
        let cert = parse_test_certificate(TEST_SELF_SIGNED_SERVER_CERT_PEM);
        let valid_name = ServerName::try_from("selfsigned.test").unwrap();
        let wrong_name = ServerName::try_from("wrong.test").unwrap();
        let now = UnixTime::since_unix_epoch(std::time::Duration::from_secs(1_800_000_000));

        let strict = test_policy_verifier(TLS_DEFAULT_VERIFY_FLAGS, RootCertStore::empty());
        assert!(
            strict
                .verify_server_cert(&cert, &[], &valid_name, &[], now)
                .is_err(),
            "an untrusted self-signed leaf must fail without the explicit option",
        );

        let relaxed = test_policy_verifier(
            TLS_DEFAULT_VERIFY_FLAGS | TLS_ALLOW_SELF_SIGNED,
            RootCertStore::empty(),
        );
        assert!(
            relaxed
                .verify_server_cert(&cert, &[], &valid_name, &[], now)
                .is_ok(),
            "a valid self-signed depth-zero leaf must be accepted",
        );
        assert!(
            relaxed
                .verify_server_cert(&cert, &[], &wrong_name, &[], now)
                .is_err(),
            "allow_self_signed must not disable peer-name verification",
        );
        assert!(
            relaxed
                .verify_server_cert(&cert, &[cert.clone()], &valid_name, &[], now)
                .is_ok(),
            "extra presented certificates must not change a depth-zero self-signed leaf",
        );

        let ca_signed_leaf = parse_test_certificate(TEST_CA_SIGNED_SERVER_CERT_PEM);
        assert!(
            relaxed
                .verify_server_cert(&ca_signed_leaf, &[], &valid_name, &[], now)
                .is_err(),
            "a CA-signed leaf without its issuer must not become self-trusted",
        );

        let chain_only = test_policy_verifier(
            TLS_VERIFY_PEER | TLS_ALLOW_SELF_SIGNED,
            RootCertStore::empty(),
        );
        assert!(
            chain_only
                .verify_server_cert(&cert, &[], &wrong_name, &[], now)
                .is_ok(),
            "verify_peer_name=false must not perform a hidden name check",
        );
    }

    /// Verifies cafile, capath, and a combined local_cert/key PEM can be
    /// configured together, including php-src's absent-local_pk fallback.
    #[test]
    fn policy_config_combines_trust_sources_and_local_cert_key_fallback() {
        let base = std::env::temp_dir().join(format!(
            "elephc_tls_combined_options_{}",
            std::process::id()
        ));
        let capath = base.join("ca");
        std::fs::create_dir_all(&capath).unwrap();
        let cafile = base.join("cafile.pem");
        std::fs::write(&cafile, TEST_SELF_SIGNED_SERVER_CERT_PEM).unwrap();
        std::fs::write(
            capath.join("anchor.pem"),
            TEST_SELF_SIGNED_SERVER_CERT_PEM,
        )
        .unwrap();
        let combined_identity = base.join("identity.pem");
        std::fs::write(
            &combined_identity,
            format!("{TEST_CLIENT_CERT_PEM}{TEST_CLIENT_KEY_PEM}"),
        )
        .unwrap();

        let config = policy_client_config(TlsOptions {
            verification_flags: TLS_DEFAULT_VERIFY_FLAGS,
            cafile: cafile.to_str(),
            capath: capath.to_str(),
            client_cert: combined_identity.to_str(),
            client_key: None,
            passphrase: None,
            alpn_protocols: None,
            peer_fingerprint: None,
            ciphers: None,
            security_level: None,
            disable_compression: None,
            option_flags: 0,
        });
        let _ = std::fs::remove_dir_all(&base);
        assert!(
            config.is_some(),
            "all trust and client-auth sources must remain combinable",
        );
    }

    /// Verifies local_pk without local_cert is ignored like php-src instead of
    /// turning on partial client authentication.
    #[test]
    fn policy_config_ignores_local_pk_without_local_cert() {
        let mut options = TlsOptions::secure_defaults();
        options.client_key = Some("/nonexistent/key-without-local-cert.pem");
        assert!(policy_client_config(options).is_some());
    }

    /// Verifies disabled chain validation does not read unused CA paths, matching php-src.
    #[test]
    fn policy_config_ignores_ca_paths_when_verify_peer_is_disabled() {
        let options = TlsOptions {
            verification_flags: TLS_VERIFY_PEER_NAME,
            cafile: Some("/nonexistent/unused-cafile.pem"),
            capath: Some("/nonexistent/unused-capath"),
            client_cert: None,
            client_key: None,
            passphrase: None,
            alpn_protocols: None,
            peer_fingerprint: None,
            ciphers: None,
            security_level: None,
            disable_compression: None,
            option_flags: 0,
        };
        assert!(policy_client_config(options).is_some());
    }

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

    /// Verifies PKCS#8 encrypted PEM loading, wrong-passphrase failure, and
    /// extraction from a certificate-plus-key combined PEM file. The command
    /// is test-only fixture generation; the production path is pure Rust.
    #[test]
    fn encrypted_pkcs8_passphrase_loads_and_rejects_wrong_secret() {
        let base = std::env::temp_dir().join(format!(
            "elephc_tls_encrypted_key_{}",
            std::process::id()
        ));
        let input = base.with_extension("input.pem");
        let output = base.with_extension("encrypted.pem");
        std::fs::write(&input, TEST_CLIENT_KEY_PEM).unwrap();
        let status = std::process::Command::new("openssl")
            .args([
                "pkcs8",
                "-topk8",
                "-in",
                input.to_str().unwrap(),
                "-out",
                output.to_str().unwrap(),
                "-passout",
                "pass:elephc-test-passphrase",
            ])
            .status();
        assert!(
            status.map(|status| status.success()).unwrap_or(false),
            "openssl is required to generate the encrypted PKCS#8 test fixture"
        );
        let encrypted = std::fs::read(&output).unwrap();
        assert!(load_private_key(&encrypted, Some("elephc-test-passphrase")).is_some());
        assert!(load_private_key(&encrypted, Some("wrong-passphrase")).is_none());
        let combined = format!("{TEST_CLIENT_CERT_PEM}{}", String::from_utf8(encrypted).unwrap());
        assert!(load_private_key(combined.as_bytes(), Some("elephc-test-passphrase")).is_some());
        let _ = std::fs::remove_file(&input);
        let _ = std::fs::remove_file(&output);
    }

    /// Verifies the legacy OpenSSL PEM envelope used by PHP's own
    /// `private_rsa_2048_pass_php.key` fixture. Missing and wrong passphrases
    /// fail closed, while the correct password produces an RSA key without
    /// invoking an OpenSSL/system decryptor.
    #[test]
    fn encrypted_legacy_pem_passphrase_loads_and_rejects_wrong_or_absent_secret() {
        let encrypted = include_bytes!("../tests/fixtures/private_rsa_2048_pass_php.key");
        assert!(load_private_key(encrypted, Some("php")).is_some());
        assert!(load_private_key(encrypted, Some("wrong")).is_none());
        assert!(load_private_key(encrypted, None).is_none());
        let mut combined = b"-----BEGIN CERTIFICATE-----\nAA==\n-----END CERTIFICATE-----\n".to_vec();
        combined.extend_from_slice(encrypted);
        assert!(load_private_key(&combined, Some("php")).is_some());
        combined.zeroize();
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

    /// Verifies the server crypto method can build a rustls `ServerConfig` from
    /// the same `local_cert`/`local_pk` option block used by PHP stream
    /// contexts. Socket adoption and handshake progress remain runtime-gated.
    #[test]
    fn server_attach_accepts_local_certificate_options() {
        let dir = std::env::temp_dir();
        let cert = dir.join(format!("elephc_tls_server_cert_{}.pem", std::process::id()));
        let key = dir.join(format!("elephc_tls_server_key_{}.pem", std::process::id()));
        std::fs::write(&cert, TEST_CLIENT_CERT_PEM).unwrap();
        std::fs::write(&key, TEST_CLIENT_KEY_PEM).unwrap();

        let cert_path = cert.to_str().unwrap();
        let key_path = key.to_str().unwrap();
        let config = server_config(TlsOptions {
            verification_flags: 0,
            cafile: None,
            capath: None,
            client_cert: Some(cert_path),
            client_key: Some(key_path),
            passphrase: None,
            alpn_protocols: Some("h2"),
            peer_fingerprint: None,
            ciphers: Some("ECDHE-RSA-AES128-GCM-SHA256"),
            security_level: None,
            disable_compression: None,
            option_flags: TLS_OPTION_NO_TICKET,
        }, PHP_STREAM_CRYPTO_TLS_1_2);
        let _ = std::fs::remove_file(&cert);
        let _ = std::fs::remove_file(&key);
        let config = config.expect("server TLS must accept a valid local cert/key pair");
        assert_eq!(config.alpn_protocols, vec![b"h2".to_vec()]);
        assert_eq!(config.send_tls13_tickets, 0);
        assert!(config
            .crypto_provider()
            .cipher_suites
            .iter()
            .any(|suite| suite.suite() == CipherSuite::TLS_ECDHE_RSA_WITH_AES_128_GCM_SHA256));
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
