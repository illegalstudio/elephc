//! Purpose:
//! libssl-backed TLS client stream backend for elephc's PHP 8.6 session
//! resumption options.
//!
//! Called from:
//! - `cargo test -p elephc-tls-openssl` with a managed OpenSSL `lib/` directory.
//! - Compiled PHP programs through the future `session_data` stream-context
//!   path; this crate is the backend foundation, not yet wired into the AOT
//!   `https://` emitter.
//!
//! Key details:
//! - The handle table owns one `SSL`/`SSL_CTX` pair plus the connected
//!   `TcpStream`; `SSL_set_fd` uses OpenSSL's no-close socket BIO, so the
//!   stream is the sole owner of the descriptor.
//! - `connect_session` imports an OpenSSL session from PEM and installs it
//!   before the handshake, which is the operation rustls cannot express.
//! - The crate never links OpenSSL at build time: the managed `openssl-crypto`
//!   package supplies the archives at the final program link, while the
//!   test-only build links a pinned `ELEPHC_OPENSSL_LIB_DIR`.

use std::cell::RefCell;
use std::collections::HashMap;
use std::ffi::{c_char, c_int, c_long, c_void, CStr, CString};
use std::net::{IpAddr, TcpStream};
use std::os::fd::AsRawFd;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::ptr;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Mutex, OnceLock};

/// Opaque libssl connection.
#[repr(C)]
pub(crate) struct SSL {
    _private: [u8; 0],
}

/// Opaque libssl context.
#[repr(C)]
pub(crate) struct SSL_CTX {
    _private: [u8; 0],
}

/// Opaque libssl method table.
#[repr(C)]
pub(crate) struct SSL_METHOD {
    _private: [u8; 0],
}

/// Opaque libssl session.
#[repr(C)]
pub(crate) struct SSL_SESSION {
    _private: [u8; 0],
}

/// Opaque libssl cipher descriptor.
#[repr(C)]
pub(crate) struct SSL_CIPHER {
    _private: [u8; 0],
}

/// Opaque libcrypto digest descriptor.
#[repr(C)]
pub(crate) struct EVP_MD {
    _private: [u8; 0],
}

/// Opaque memory BIO.
#[repr(C)]
pub(crate) struct BIO {
    _private: [u8; 0],
}

/// Opaque BIO method table returned by `BIO_s_mem`.
#[repr(C)]
pub(crate) struct BIO_METHOD {
    _private: [u8; 0],
}

/// `SSL_VERIFY_NONE`.
const SSL_VERIFY_NONE: c_int = 0;
/// `SSL_VERIFY_PEER`.
const SSL_VERIFY_PEER: c_int = 1;
/// `SSL_FILETYPE_PEM`.
const SSL_FILETYPE_PEM: c_int = 1;
/// `SSL_ERROR_WANT_READ`.
const SSL_ERROR_WANT_READ: c_int = 2;
/// `SSL_ERROR_WANT_WRITE`.
const SSL_ERROR_WANT_WRITE: c_int = 3;
/// `SSL_ERROR_SYSCALL`.
const SSL_ERROR_SYSCALL: c_int = 5;
/// `SSL_ERROR_ZERO_RETURN`.
const SSL_ERROR_ZERO_RETURN: c_int = 6;
/// `BIO_CTRL_INFO`, which returns the memory BIO's length and data pointer.
const BIO_CTRL_INFO: c_int = 3;
/// `SSL_CTRL_SET_TLSEXT_HOSTNAME`.
const SSL_CTRL_SET_TLSEXT_HOSTNAME: c_int = 55;
/// `TLSEXT_NAMETYPE_host_name`.
const TLSEXT_NAMETYPE_HOST_NAME: c_long = 0;
/// `SSL_CTRL_SET_MAX_PROTO_VERSION`.
const SSL_CTRL_SET_MAX_PROTO_VERSION: c_int = 124;
/// `SSL_CTRL_SET_SESS_CACHE_MODE`.
const SSL_CTRL_SET_SESS_CACHE_MODE: c_int = 44;
/// `SSL_SESS_CACHE_CLIENT`.
const SSL_SESS_CACHE_CLIENT: c_long = 0x0001;
/// `SSL_SESS_CACHE_NO_INTERNAL`.
const SSL_SESS_CACHE_NO_INTERNAL: c_long = 0x0300;
/// `SSL_SESS_CACHE_SERVER`.
#[cfg(all(test, elephc_tls_openssl_native))]
const SSL_SESS_CACHE_SERVER: c_long = 2;
/// `TLS1_2_VERSION`.
#[cfg(all(test, elephc_tls_openssl_native))]
pub(crate) const TLS1_2_VERSION: c_long = 0x0303;
/// `TLS1_3_VERSION`.
const TLS1_3_VERSION: c_int = 0x0304;
/// TLS 1.3 cipher-suite id for `TLS_AES_128_GCM_SHA256`.
const TLS1_3_AES_128_GCM_SHA256: [u8; 2] = [0x13, 0x01];
/// TLS 1.3 cipher-suite id for `TLS_AES_256_GCM_SHA384`.
const TLS1_3_AES_256_GCM_SHA384: [u8; 2] = [0x13, 0x02];
/// `OPENSSL_ENCODING_PEM`-style result kind the PHP adapter uses for a string.
const PSK_RESULT_STRING: i64 = 1;

extern "C" {
    fn OPENSSL_init_ssl(opts: u64, settings: *const c_void) -> c_int;
    fn TLS_client_method() -> *const SSL_METHOD;
    fn SSL_CTX_new(method: *const SSL_METHOD) -> *mut SSL_CTX;
    fn SSL_CTX_free(ctx: *mut SSL_CTX);
    fn SSL_CTX_set_verify(
        ctx: *mut SSL_CTX,
        mode: c_int,
        callback: Option<unsafe extern "C" fn(c_int, *mut c_void) -> c_int>,
    );
    fn SSL_CTX_set_default_verify_paths(ctx: *mut SSL_CTX) -> c_int;
    fn SSL_CTX_load_verify_locations(
        ctx: *mut SSL_CTX,
        cafile: *const c_char,
        capath: *const c_char,
    ) -> c_int;
    fn SSL_CTX_use_certificate_file(
        ctx: *mut SSL_CTX,
        file: *const c_char,
        file_type: c_int,
    ) -> c_int;
    fn SSL_CTX_use_PrivateKey_file(
        ctx: *mut SSL_CTX,
        file: *const c_char,
        file_type: c_int,
    ) -> c_int;
    fn SSL_CTX_ctrl(ctx: *mut SSL_CTX, cmd: c_int, larg: c_long, parg: *mut c_void) -> c_long;
    fn SSL_new(ctx: *mut SSL_CTX) -> *mut SSL;
    fn SSL_free(ssl: *mut SSL);
    fn SSL_set_fd(ssl: *mut SSL, fd: c_int) -> c_int;
    fn SSL_ctrl(ssl: *mut SSL, cmd: c_int, larg: c_long, parg: *mut c_void) -> c_long;
    fn SSL_set1_host(ssl: *mut SSL, hostname: *const c_char) -> c_int;
    fn SSL_set_session(ssl: *mut SSL, session: *mut SSL_SESSION) -> c_int;
    fn SSL_get_session(ssl: *const SSL) -> *mut SSL_SESSION;
    fn SSL_session_reused(ssl: *const SSL) -> c_int;
    fn SSL_connect(ssl: *mut SSL) -> c_int;
    fn SSL_read(ssl: *mut SSL, buf: *mut c_void, num: c_int) -> c_int;
    fn SSL_write(ssl: *mut SSL, buf: *const c_void, num: c_int) -> c_int;
    fn SSL_shutdown(ssl: *mut SSL) -> c_int;
    fn SSL_get_error(ssl: *const SSL, ret: c_int) -> c_int;
    fn SSL_SESSION_free(session: *mut SSL_SESSION);
    fn PEM_read_bio_SSL_SESSION(
        bio: *mut BIO,
        session: *mut *mut SSL_SESSION,
        callback: Option<unsafe extern "C" fn(*mut c_char, c_int, c_int, *mut c_void) -> c_int>,
        userdata: *mut c_void,
    ) -> *mut SSL_SESSION;
    fn PEM_write_bio_SSL_SESSION(bio: *mut BIO, session: *const SSL_SESSION) -> c_int;
    fn BIO_new_mem_buf(buf: *const c_void, len: c_int) -> *mut BIO;
    fn BIO_new(method: *const BIO_METHOD) -> *mut BIO;
    fn BIO_s_mem() -> *const BIO_METHOD;
    fn BIO_free(bio: *mut BIO) -> c_int;
    fn BIO_ctrl(bio: *mut BIO, cmd: c_int, larg: c_long, parg: *mut c_void) -> c_long;
    fn ERR_error_string_n(code: u64, buf: *mut c_char, len: usize);
    fn ERR_get_error() -> u64;
    fn ERR_clear_error();
    fn SSL_CTX_set_psk_use_session_callback(
        ctx: *mut SSL_CTX,
        callback: Option<unsafe extern "C" fn(
            *mut SSL,
            *const EVP_MD,
            *mut *const u8,
            *mut usize,
            *mut *mut SSL_SESSION,
        ) -> c_int>,
    );
    fn SSL_CTX_set_psk_client_callback(
        ctx: *mut SSL_CTX,
        callback: Option<unsafe extern "C" fn(
            *mut SSL,
            *const c_char,
            *mut c_char,
            u32,
            *mut u8,
            u32,
        ) -> u32>,
    );
    fn SSL_SESSION_new() -> *mut SSL_SESSION;
    fn SSL_SESSION_set1_master_key(
        session: *mut SSL_SESSION,
        key: *const u8,
        len: usize,
    ) -> c_int;
    fn SSL_SESSION_set_cipher(session: *mut SSL_SESSION, cipher: *const SSL_CIPHER) -> c_int;
    fn SSL_SESSION_set_protocol_version(session: *mut SSL_SESSION, version: c_int) -> c_int;
    fn SSL_SESSION_set_max_early_data(session: *mut SSL_SESSION, max_early_data: u32) -> c_int;
    fn SSL_SESSION_get_max_early_data(session: *const SSL_SESSION) -> u32;
    fn SSL_CIPHER_find(ssl: *mut SSL, cipher_id: *const u8) -> *const SSL_CIPHER;
    fn SSL_CIPHER_get_handshake_digest(cipher: *const SSL_CIPHER) -> *const EVP_MD;
    fn SSL_write_early_data(
        ssl: *mut SSL,
        buffer: *const c_void,
        length: usize,
        written: *mut usize,
    ) -> c_int;
    fn SSL_get_early_data_status(ssl: *const SSL) -> c_int;
    fn SSL_CTX_sess_set_new_cb(
        ctx: *mut SSL_CTX,
        callback: Option<unsafe extern "C" fn(*mut SSL, *mut SSL_SESSION) -> c_int>,
    );
}

#[cfg(all(test, elephc_tls_openssl_native))]
extern "C" {
    fn TLS_server_method() -> *const SSL_METHOD;
    fn SSL_CTX_set_session_id_context(
        ctx: *mut SSL_CTX,
        sid_ctx: *const u8,
        sid_ctx_len: u32,
    ) -> c_int;
    fn SSL_accept(ssl: *mut SSL) -> c_int;
    fn SSL_CTX_set_max_early_data(ctx: *mut SSL_CTX, max_early_data: u32) -> c_int;
    fn SSL_CTX_set_recv_max_early_data(ctx: *mut SSL_CTX, max_early_data: u32) -> c_int;
    fn SSL_read_early_data(
        ssl: *mut SSL,
        buffer: *mut c_void,
        length: usize,
        readbytes: *mut usize,
    ) -> c_int;
    fn SSL_CTX_set_psk_find_session_callback(
        ctx: *mut SSL_CTX,
        callback: Option<unsafe extern "C" fn(
            *mut SSL,
            *const u8,
            usize,
            *mut *mut SSL_SESSION,
        ) -> c_int>,
    );
    fn SSL_CTX_set_ciphersuites(ctx: *mut SSL_CTX, suites: *const c_char) -> c_int;
}

/// One live TLS connection.
struct Entry {
    ssl: usize,
    ctx: usize,
    stream: TcpStream,
    /// `SSL_get_early_data_status` captured before the handshake handle is
    /// published: 0 = not sent, 1 = rejected, 2 = accepted.
    early_data_status: i64,
}

// The raw pointers are only dereferenced while the handle-table mutex is held;
// the stream keeps the descriptor alive for the SSL object's whole lifetime.
unsafe impl Send for Entry {}

static HANDLES: OnceLock<Mutex<HashMap<i64, Entry>>> = OnceLock::new();
static NEXT_ID: AtomicI64 = AtomicI64::new(1);

thread_local! {
    static LAST_ERROR: RefCell<CString> = RefCell::new(CString::default());
    static SESSION_PEM: RefCell<CString> = RefCell::new(CString::default());
    /// The PSK callback installed on the connection currently handshaking.
    static PSK_CALLBACK: RefCell<Option<PskCallback>> = const { RefCell::new(None) };
    /// Keeps the last identity advertised by the callback alive until the
    /// handshake has consumed it.
    static PSK_IDENTITY: RefCell<Vec<u8>> = const { RefCell::new(Vec::new()) };
    /// Maximum early data advertised by the PSK session built for this
    /// connection; 0 disables 0-RTT.
    static PSK_MAX_EARLY_DATA: RefCell<u32> = const { RefCell::new(0) };
}

/// One PHP-supplied session-new callback, decomposed at the PHP layer.
#[derive(Clone, Copy)]
struct SessionCallback {
    descriptor: *mut c_void,
    adapter: unsafe extern "C" fn(*mut c_void, *mut c_void, *mut PskCallSpec) -> i64,
}

// The descriptor is owned and rooted by compiled PHP, and the adapter is a
// process-wide code address. The bridge only stores/calls them while a
// handshake is active on the connecting thread.
unsafe impl Send for SessionCallback {}

/// The process-wide `ssl.session_new_cb` callback.
static SESSION_CALLBACK: OnceLock<Mutex<Option<SessionCallback>>> = OnceLock::new();

/// Returns the process-wide session-new callback slot.
fn session_callback() -> &'static Mutex<Option<SessionCallback>> {
    SESSION_CALLBACK.get_or_init(|| Mutex::new(None))
}

/// One PHP-supplied external-PSK callback, decomposed at the PHP layer.
#[derive(Clone, Copy)]
struct PskCallback {
    /// The normalized callable descriptor pointer.
    descriptor: *mut c_void,
    /// The codegen adapter address (`__rt_curl_invoke_callback`).
    adapter: unsafe extern "C" fn(*mut c_void, *mut c_void, *mut PskCallSpec) -> i64,
}

/// One PHP argument in `__rt_mixed_from_value`'s `(tag, lo, hi)` encoding.
#[repr(C)]
pub(crate) struct PskCallArg {
    tag: i64,
    lo: i64,
    hi: i64,
}

/// The bridge-owned call descriptor the codegen adapter reads at fixed offsets.
///
/// Layout is load-bearing: it must match `crates/elephc-curl/src/callbacks.rs`'s
/// `CallSpec` because the shared adapter is `__rt_curl_invoke_callback`.
#[repr(C)]
pub(crate) struct PskCallSpec {
    argc: i64,
    argv: *const PskCallArg,
    result_kind: i64,
    out_buf: *mut u8,
    out_cap: i64,
    out_len: i64,
    status: i64,
}

/// Returns the process-wide handle table.
fn handles() -> &'static Mutex<HashMap<i64, Entry>> {
    HANDLES.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Locks the handle table, recovering from a poisoned mutex.
fn lock_handles() -> std::sync::MutexGuard<'static, HashMap<i64, Entry>> {
    handles().lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Stores the last bridge error as a thread-local C string.
fn record_error(message: impl Into<String>) {
    let text = message.into();
    LAST_ERROR.with(|slot| {
        *slot.borrow_mut() = CString::new(text).unwrap_or_default();
    });
}

/// Drains libssl's error queue into the thread-local error slot.
fn record_ssl_error(prefix: &str) {
    let mut message = String::new();
    loop {
        let code = unsafe { ERR_get_error() };
        if code == 0 {
            break;
        }
        let mut buffer = [0 as c_char; 256];
        unsafe { ERR_error_string_n(code, buffer.as_mut_ptr(), buffer.len()) };
        let text = unsafe { CStr::from_ptr(buffer.as_ptr()) }.to_string_lossy();
        if !message.is_empty() {
            message.push_str("; ");
        }
        message.push_str(&text);
        if message.len() > 768 {
            break;
        }
    }
    if message.is_empty() {
        message.push_str(prefix);
    }
    record_error(message);
}

/// Clears the thread-local bridge error.
fn clear_error() {
    unsafe { ERR_clear_error() };
    LAST_ERROR.with(|slot| *slot.borrow_mut() = CString::default());
}

/// Initializes libssl once for this process.
fn init_ssl() {
    static INIT: OnceLock<()> = OnceLock::new();
    INIT.get_or_init(|| unsafe {
        let _ = OPENSSL_init_ssl(0, ptr::null());
    });
}

/// Creates a client context, or `None` after recording the bridge error.
unsafe fn create_client_ctx(
    insecure: bool,
    cafile: Option<&str>,
    client_cert: Option<(&str, &str)>,
    max_version: Option<c_long>,
) -> Option<*mut SSL_CTX> {
    let ctx = SSL_CTX_new(TLS_client_method());
    if ctx.is_null() {
        record_ssl_error("Failed to create TLS context");
        return None;
    }
    SSL_CTX_set_verify(
        ctx,
        if insecure {
            SSL_VERIFY_NONE
        } else {
            SSL_VERIFY_PEER
        },
        None,
    );
    if let Some(cafile) = cafile {
        let Ok(path) = CString::new(cafile) else {
            record_error("TLS CA file name contains NUL");
            SSL_CTX_free(ctx);
            return None;
        };
        if SSL_CTX_load_verify_locations(ctx, path.as_ptr(), ptr::null()) != 1 {
            record_ssl_error("Failed to load TLS CA file");
            SSL_CTX_free(ctx);
            return None;
        }
    } else if !insecure && SSL_CTX_set_default_verify_paths(ctx) != 1 {
        record_ssl_error("Failed to load default TLS trust roots");
        SSL_CTX_free(ctx);
        return None;
    }
    if let Some((cert, key)) = client_cert {
        let (Ok(cert), Ok(key)) = (CString::new(cert), CString::new(key)) else {
            record_error("TLS client certificate path contains NUL");
            SSL_CTX_free(ctx);
            return None;
        };
        if SSL_CTX_use_certificate_file(ctx, cert.as_ptr(), SSL_FILETYPE_PEM) != 1
            || SSL_CTX_use_PrivateKey_file(ctx, key.as_ptr(), SSL_FILETYPE_PEM) != 1
        {
            record_ssl_error("Failed to load TLS client certificate");
            SSL_CTX_free(ctx);
            return None;
        }
    }
    if let Some(version) = max_version {
        if SSL_CTX_ctrl(
            ctx,
            SSL_CTRL_SET_MAX_PROTO_VERSION,
            version,
            ptr::null_mut(),
        ) != 1
        {
            record_ssl_error("Failed to set TLS protocol version");
            SSL_CTX_free(ctx);
            return None;
        }
    }
    if session_callback()
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .is_some()
    {
        SSL_CTX_sess_set_new_cb(ctx, Some(session_new_cb));
        SSL_CTX_ctrl(
            ctx,
            SSL_CTRL_SET_SESS_CACHE_MODE,
            SSL_SESS_CACHE_CLIENT | SSL_SESS_CACHE_NO_INTERNAL,
            ptr::null_mut(),
        );
    }
    Some(ctx)
}

/// Loads a PEM session into a new `SSL_SESSION`, or `None` on failure.
unsafe fn parse_session(pem: &[u8]) -> Option<*mut SSL_SESSION> {
    if pem.is_empty() || pem.len() > c_int::MAX as usize {
        return None;
    }
    let bio = BIO_new_mem_buf(pem.as_ptr() as *const c_void, pem.len() as c_int);
    if bio.is_null() {
        return None;
    }
    let session = PEM_read_bio_SSL_SESSION(bio, ptr::null_mut(), None, ptr::null_mut());
    BIO_free(bio);
    if session.is_null() {
        None
    } else {
        Some(session)
    }
}

/// OpenSSL new-session callback: exports the negotiated session to PEM and
/// invokes the PHP `ssl.session_new_cb` callable through the shared adapter.
///
/// # Safety
/// Called by libssl during/after a handshake on the connecting thread.
unsafe extern "C" fn session_new_cb(_ssl: *mut SSL, session: *mut SSL_SESSION) -> c_int {
    let Some(callback) = *session_callback().lock().unwrap_or_else(|p| p.into_inner()) else {
        return 0;
    };
    if session.is_null() {
        return 0;
    }
    let bio = BIO_new(BIO_s_mem());
    if bio.is_null() {
        return 0;
    }
    if PEM_write_bio_SSL_SESSION(bio, session) != 1 {
        BIO_free(bio);
        return 0;
    }
    let mut data: *mut c_char = ptr::null_mut();
    let length = BIO_ctrl(
        bio,
        BIO_CTRL_INFO,
        0,
        &mut data as *mut *mut c_char as *mut c_void,
    );
    let pem = if length > 0 && !data.is_null() {
        std::slice::from_raw_parts(data as *const u8, length as usize).to_vec()
    } else {
        Vec::new()
    };
    BIO_free(bio);
    if pem.is_empty() {
        return 0;
    }
    let argument = PskCallArg {
        tag: 1,
        lo: pem.as_ptr() as i64,
        hi: pem.len() as i64,
    };
    let mut spec = PskCallSpec {
        argc: 1,
        argv: &argument as *const PskCallArg,
        result_kind: 0,
        out_buf: ptr::null_mut(),
        out_cap: 0,
        out_len: 0,
        status: 0,
    };
    let _ = (callback.adapter)(callback.descriptor, ptr::null_mut(), &mut spec);
    0
}

/// C ABI: installs or clears the PHP `ssl.session_new_cb` callback.
#[no_mangle]
pub extern "C" fn elephc_tls_set_session_new_cb(
    descriptor: *mut c_void,
    adapter: *const c_void,
) {
    let callback = if descriptor.is_null() || adapter.is_null() {
        None
    } else {
        Some(SessionCallback {
            descriptor,
            adapter: unsafe {
                std::mem::transmute::<
                    *const c_void,
                    unsafe extern "C" fn(*mut c_void, *mut c_void, *mut PskCallSpec) -> i64,
                >(adapter)
            },
        })
    };
    *session_callback().lock().unwrap_or_else(|p| p.into_inner()) = callback;
}

/// Invokes the installed PHP PSK callback and returns its `(identity, psk)`
/// Invokes the installed PHP PSK callback and returns its `(identity, psk)`
/// byte pair, or `None` for a null/empty/failed result.
///
/// The PHP side returns `identity_len (4 bytes, big-endian) | identity | psk`;
/// the adapter is the shared `__rt_curl_invoke_callback`, so the spec layout must
/// stay exactly the curl bridge's `CallSpec`.
unsafe fn invoke_psk_callback(identity: Option<&[u8]>) -> Option<(Vec<u8>, Vec<u8>)> {
    let callback = PSK_CALLBACK.with(|slot| *slot.borrow())?;
    let argument = identity.map(|bytes| PskCallArg {
        tag: 1,
        lo: bytes.as_ptr() as i64,
        hi: bytes.len() as i64,
    });
    let mut buffer = vec![0u8; 512];
    let mut spec = PskCallSpec {
        argc: i64::from(argument.is_some()),
        argv: argument
            .as_ref()
            .map_or(ptr::null(), |argument| argument as *const PskCallArg),
        result_kind: PSK_RESULT_STRING,
        out_buf: buffer.as_mut_ptr(),
        out_cap: buffer.len() as i64,
        out_len: 0,
        status: 0,
    };
    let _ = (callback.adapter)(callback.descriptor, ptr::null_mut(), &mut spec);
    if spec.status != 0 || spec.out_len <= 4 || spec.out_len > spec.out_cap {
        return None;
    }
    let encoded = &buffer[..spec.out_len as usize];
    let identity_len =
        u32::from_be_bytes([encoded[0], encoded[1], encoded[2], encoded[3]]) as usize;
    if identity_len == 0 || identity_len > encoded.len() - 4 {
        return None;
    }
    let identity = encoded[4..4 + identity_len].to_vec();
    let psk = encoded[4 + identity_len..].to_vec();
    if psk.is_empty() {
        return None;
    }
    Some((identity, psk))
}

/// Builds the TLS 1.3 `SSL_SESSION` that carries an external PSK.
///
/// Mirrors php-src's `php_openssl_psk_build_session`: the session's master key
/// is the PSK, the cipher is the TLS 1.3 suite whose handshake digest matches
/// `md` when libssl supplies one, and the protocol version is TLS 1.3.
unsafe fn build_psk_session(
    ssl: *mut SSL,
    psk: &[u8],
    md: *const EVP_MD,
) -> Option<*mut SSL_SESSION> {
    let mut cipher = SSL_CIPHER_find(ssl, TLS1_3_AES_128_GCM_SHA256.as_ptr());
    if !md.is_null()
        && !cipher.is_null()
        && SSL_CIPHER_get_handshake_digest(cipher) != md
    {
        cipher = SSL_CIPHER_find(ssl, TLS1_3_AES_256_GCM_SHA384.as_ptr());
        if cipher.is_null() || SSL_CIPHER_get_handshake_digest(cipher) != md {
            return None;
        }
    }
    if cipher.is_null() {
        return None;
    }
    let session = SSL_SESSION_new();
    if session.is_null() {
        return None;
    }
    if SSL_SESSION_set1_master_key(session, psk.as_ptr(), psk.len()) != 1
        || SSL_SESSION_set_cipher(session, cipher) != 1
        || SSL_SESSION_set_protocol_version(session, TLS1_3_VERSION) != 1
    {
        SSL_SESSION_free(session);
        return None;
    }
    let max_early_data = PSK_MAX_EARLY_DATA.with(|slot| *slot.borrow());
    if max_early_data > 0 && SSL_SESSION_set_max_early_data(session, max_early_data) != 1 {
        SSL_SESSION_free(session);
        return None;
    }
    Some(session)
}

/// TLS 1.3 external-PSK client callback.
///
/// # Safety
/// Called by libssl with writable `identity_out`, `identity_len_out`, and
/// `session_out` pointers for the duration of the handshake.
unsafe extern "C" fn psk_use_session_cb(
    ssl: *mut SSL,
    md: *const EVP_MD,
    identity_out: *mut *const u8,
    identity_len_out: *mut usize,
    session_out: *mut *mut SSL_SESSION,
) -> c_int {
    let Some((identity, psk)) = invoke_psk_callback(None) else {
        return 0;
    };
    let Some(session) = build_psk_session(ssl, &psk, md) else {
        record_error("Failed to build the TLS 1.3 PSK session");
        return 0;
    };
    PSK_IDENTITY.with(|slot| *slot.borrow_mut() = identity);
    let (pointer, length) = PSK_IDENTITY.with(|slot| {
        let identity = slot.borrow();
        (identity.as_ptr(), identity.len())
    });
    *identity_out = pointer;
    *identity_len_out = length;
    *session_out = session;
    1
}

/// TLS 1.2 external-PSK client callback.
///
/// # Safety
/// Called by libssl with `identity_out` sized for `max_identity_len` bytes and
/// `psk_out` sized for `max_psk_len` bytes.
unsafe extern "C" fn psk_client_cb(
    _ssl: *mut SSL,
    _hint: *const c_char,
    identity_out: *mut c_char,
    max_identity_len: u32,
    psk_out: *mut u8,
    max_psk_len: u32,
) -> u32 {
    let Some((identity, psk)) = invoke_psk_callback(None) else {
        return 0;
    };
    if identity.is_empty()
        || identity.len() + 1 > max_identity_len as usize
        || psk.len() > max_psk_len as usize
    {
        record_error("TLS PSK identity or key exceeds the libssl buffer");
        return 0;
    }
    ptr::copy_nonoverlapping(identity.as_ptr(), identity_out as *mut u8, identity.len());
    *identity_out.add(identity.len()) = 0;
    ptr::copy_nonoverlapping(psk.as_ptr(), psk_out, psk.len());
    psk.len() as u32
}

/// Opens one TCP+TLS connection and returns its handle, or `-1`.
unsafe fn connect_inner(
    host: &str,
    port: u16,
    insecure: bool,
    cafile: Option<&str>,
    client_cert: Option<(&str, &str)>,
    session: Option<&[u8]>,
    max_version: Option<c_long>,
    psk: Option<PskCallback>,
    mut early_data: Option<&[u8]>,
) -> i64 {
    init_ssl();
    clear_error();
    let Some(ctx) = create_client_ctx(insecure, cafile, client_cert, max_version) else {
        return -1;
    };
    if psk.is_some() {
        SSL_CTX_set_psk_use_session_callback(ctx, Some(psk_use_session_cb));
        SSL_CTX_set_psk_client_callback(ctx, Some(psk_client_cb));
    }
    let ssl = SSL_new(ctx);
    if ssl.is_null() {
        record_ssl_error("Failed to allocate TLS connection");
        SSL_CTX_free(ctx);
        return -1;
    }
    let stream = match TcpStream::connect((host, port)) {
        Ok(stream) => stream,
        Err(error) => {
            record_error(format!("TLS TCP connect failed: {error}"));
            SSL_free(ssl);
            SSL_CTX_free(ctx);
            return -1;
        }
    };
    if SSL_set_fd(ssl, stream.as_raw_fd()) != 1 {
        record_ssl_error("Failed to attach TLS socket");
        SSL_free(ssl);
        SSL_CTX_free(ctx);
        return -1;
    }
    let Ok(host_cstr) = CString::new(host) else {
        record_error("TLS host name contains NUL");
        SSL_free(ssl);
        SSL_CTX_free(ctx);
        return -1;
    };
    let is_ip = host.parse::<IpAddr>().is_ok();
    if !is_ip {
        SSL_ctrl(
            ssl,
            SSL_CTRL_SET_TLSEXT_HOSTNAME,
            TLSEXT_NAMETYPE_HOST_NAME,
            host_cstr.as_ptr() as *mut c_void,
        );
    }
    if !insecure && SSL_set1_host(ssl, host_cstr.as_ptr()) != 1 {
        record_ssl_error("Failed to configure TLS hostname verification");
        SSL_free(ssl);
        SSL_CTX_free(ctx);
        return -1;
    }
    if let Some(pem) = session {
        let Some(session) = parse_session(pem) else {
            record_ssl_error("Failed to import TLS session");
            SSL_free(ssl);
            SSL_CTX_free(ctx);
            return -1;
        };
        if SSL_set_session(ssl, session) != 1 {
            record_ssl_error("Failed to install TLS session");
            SSL_SESSION_free(session);
            SSL_free(ssl);
            SSL_CTX_free(ctx);
            return -1;
        }
        if SSL_SESSION_get_max_early_data(session) == 0 {
            early_data = None;
        }
        SSL_SESSION_free(session);
    }
    let previous_psk = if psk.is_some() {
        Some(PSK_CALLBACK.with(|slot| slot.replace(psk)))
    } else {
        None
    };
    let previous_max_early = PSK_MAX_EARLY_DATA.with(|slot| {
        slot.replace(early_data.map_or(0, |data| data.len() as u32))
    });
    if let Some(data) = early_data {
        let mut written = 0usize;
        while written < data.len() {
            let mut chunk = 0usize;
            let sent = SSL_write_early_data(
                ssl,
                data[written..].as_ptr() as *const c_void,
                data.len() - written,
                &mut chunk,
            );
            if sent != 1 {
                record_ssl_error("Failed to send TLS early data");
                PSK_CALLBACK.with(|slot| *slot.borrow_mut() = previous_psk.unwrap_or(None));
                PSK_MAX_EARLY_DATA.with(|slot| *slot.borrow_mut() = previous_max_early);
                SSL_free(ssl);
                SSL_CTX_free(ctx);
                return -1;
            }
            if chunk == 0 {
                break;
            }
            written += chunk;
        }
    }
    let rc = SSL_connect(ssl);
    let early_status = SSL_get_early_data_status(ssl);
    if let Some(previous) = previous_psk {
        PSK_CALLBACK.with(|slot| *slot.borrow_mut() = previous);
    }
    PSK_MAX_EARLY_DATA.with(|slot| *slot.borrow_mut() = previous_max_early);
    if rc != 1 {
        let error = SSL_get_error(ssl, rc);
        if error == SSL_ERROR_SYSCALL {
            record_error(format!(
                "TLS handshake failed: {}",
                std::io::Error::last_os_error()
            ));
        } else {
            record_ssl_error("TLS handshake failed");
        }
        SSL_free(ssl);
        SSL_CTX_free(ctx);
        return -1;
    }
    let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
    lock_handles().insert(
        id,
        Entry {
            ssl: ssl as usize,
            ctx: ctx as usize,
            stream,
            early_data_status: early_status as i64,
        },
    );
    id
}

/// C ABI: returns the backend version.
#[no_mangle]
pub extern "C" fn elephc_tls_openssl_version() -> i32 {
    catch_unwind(AssertUnwindSafe(|| 1)).unwrap_or(1)
}

/// C ABI: connects with default trust roots and hostname verification.
///
/// # Safety
/// `host` must point at `host_len` readable bytes.
#[no_mangle]
pub unsafe extern "C" fn elephc_tls_openssl_connect(
    host: *const u8,
    host_len: usize,
    port: u16,
) -> i64 {
    let host = std::slice::from_raw_parts(host, host_len);
    let Ok(host) = std::str::from_utf8(host) else {
        record_error("TLS host is not UTF-8");
        return -1;
    };
    catch_unwind(AssertUnwindSafe(|| {
        connect_inner(host, port, false, None, None, None, None, None, None)
    }))
    .unwrap_or(-1)
}

/// C ABI: connects without certificate or hostname verification.
///
/// # Safety
/// `host` must point at `host_len` readable bytes.
#[no_mangle]
pub unsafe extern "C" fn elephc_tls_openssl_connect_insecure(
    host: *const u8,
    host_len: usize,
    port: u16,
) -> i64 {
    let host = std::slice::from_raw_parts(host, host_len);
    let Ok(host) = std::str::from_utf8(host) else {
        record_error("TLS host is not UTF-8");
        return -1;
    };
    catch_unwind(AssertUnwindSafe(|| {
        connect_inner(host, port, true, None, None, None, None, None, None)
    }))
    .unwrap_or(-1)
}

/// C ABI: connects with a custom CA bundle and hostname verification.
///
/// # Safety
/// The host and cafile pointers must reference their declared lengths.
#[no_mangle]
pub unsafe extern "C" fn elephc_tls_openssl_connect_cafile(
    host: *const u8,
    host_len: usize,
    port: u16,
    cafile: *const u8,
    cafile_len: usize,
) -> i64 {
    let host = std::slice::from_raw_parts(host, host_len);
    let cafile = std::slice::from_raw_parts(cafile, cafile_len);
    let (Ok(host), Ok(cafile)) = (
        std::str::from_utf8(host),
        std::str::from_utf8(cafile),
    ) else {
        record_error("TLS host or CA path is not UTF-8");
        return -1;
    };
    catch_unwind(AssertUnwindSafe(|| {
        connect_inner(host, port, false, Some(cafile), None, None, None, None, None)
    }))
    .unwrap_or(-1)
}

/// C ABI: connects with an imported PEM session and default verification.
///
/// # Safety
/// The host and session pointers must reference their declared lengths.
#[no_mangle]
pub unsafe extern "C" fn elephc_tls_openssl_connect_session(
    host: *const u8,
    host_len: usize,
    port: u16,
    session: *const u8,
    session_len: usize,
    early_ptr: *const u8,
    early_len: usize,
) -> i64 {
    let host = std::slice::from_raw_parts(host, host_len);
    let session = std::slice::from_raw_parts(session, session_len);
    let Ok(host) = std::str::from_utf8(host) else {
        record_error("TLS host is not UTF-8");
        return -1;
    };
    let early = if early_ptr.is_null() || early_len == 0 {
        None
    } else {
        Some(std::slice::from_raw_parts(early_ptr, early_len))
    };
    catch_unwind(AssertUnwindSafe(|| {
        connect_inner(host, port, false, None, None, Some(session), None, None, early)
    }))
    .unwrap_or(-1)
}

/// C ABI: connects with an imported PEM session without certificate checks.
///
/// This is the test/live-local variant; the PHP stream wiring will select the
/// verification policy from the context options, not from the session presence.
///
/// # Safety
/// The host and session pointers must reference their declared lengths.
#[no_mangle]
pub unsafe extern "C" fn elephc_tls_openssl_connect_session_insecure(
    host: *const u8,
    host_len: usize,
    port: u16,
    session: *const u8,
    session_len: usize,
    early_ptr: *const u8,
    early_len: usize,
) -> i64 {
    let host = std::slice::from_raw_parts(host, host_len);
    let session = std::slice::from_raw_parts(session, session_len);
    let Ok(host) = std::str::from_utf8(host) else {
        record_error("TLS host is not UTF-8");
        return -1;
    };
    let early = if early_ptr.is_null() || early_len == 0 {
        None
    } else {
        Some(std::slice::from_raw_parts(early_ptr, early_len))
    };
    catch_unwind(AssertUnwindSafe(|| {
        connect_inner(host, port, true, None, None, Some(session), None, None, early)
    }))
    .unwrap_or(-1)
}

/// C ABI: connects with an external PSK supplied by a PHP callback.
///
/// `descriptor` is the normalized callable descriptor pointer and `adapter` the
/// address of the shared codegen adapter that re-enters compiled PHP. `insecure`
/// disables certificate verification, as the PHP `verify_peer = "0"` option does.
///
/// # Safety
/// The host pointer must reference `host_len` readable bytes; `descriptor` and
/// `adapter` must outlive the handshake and come from the PHP prelude.
#[no_mangle]
pub unsafe extern "C" fn elephc_tls_openssl_connect_psk(
    host: *const u8,
    host_len: usize,
    port: u16,
    descriptor: *mut c_void,
    adapter: *const c_void,
    insecure: c_int,
    early_ptr: *const u8,
    early_len: usize,
) -> i64 {
    let host = std::slice::from_raw_parts(host, host_len);
    let Ok(host) = std::str::from_utf8(host) else {
        record_error("TLS host is not UTF-8");
        return -1;
    };
    if descriptor.is_null() || adapter.is_null() {
        record_error("TLS PSK callback is not installed");
        return -1;
    }
    let adapter: unsafe extern "C" fn(*mut c_void, *mut c_void, *mut PskCallSpec) -> i64 =
        std::mem::transmute(adapter);
    let early_data = if early_ptr.is_null() || early_len == 0 {
        None
    } else {
        Some(std::slice::from_raw_parts(early_ptr, early_len))
    };
    catch_unwind(AssertUnwindSafe(|| {
        connect_inner(
            host,
            port,
            insecure != 0,
            None,
            None,
            None,
            None,
            Some(PskCallback {
                descriptor,
                adapter,
            }),
            early_data,
        )
    }))
    .unwrap_or(-1)
}

/// C ABI: reads plaintext, returning bytes, `0` at EOF, or `-1`.
///
/// # Safety
/// `buffer` must reference `length` writable bytes.
#[no_mangle]
pub unsafe extern "C" fn elephc_tls_openssl_read(
    handle_id: i64,
    buffer: *mut u8,
    length: usize,
) -> isize {
    if buffer.is_null() || length == 0 {
        return 0;
    }
    catch_unwind(AssertUnwindSafe(|| {
        let mut guard = lock_handles();
        let Some(entry) = guard.get_mut(&handle_id) else {
            return -1;
        };
        let ssl = entry.ssl as *mut SSL;
        let length = length.min(c_int::MAX as usize) as c_int;
        loop {
            let read = SSL_read(ssl, buffer as *mut c_void, length);
            if read > 0 {
                return read as isize;
            }
            match SSL_get_error(ssl, read) {
                SSL_ERROR_ZERO_RETURN => return 0,
                SSL_ERROR_WANT_READ | SSL_ERROR_WANT_WRITE => continue,
                SSL_ERROR_SYSCALL if ERR_get_error() == 0 => return 0,
                _ => {
                    record_ssl_error("TLS read failed");
                    return -1;
                }
            }
        }
    }))
    .unwrap_or(-1)
}

/// C ABI: writes plaintext, returning bytes written or `-1`.
///
/// # Safety
/// `buffer` must reference `length` readable bytes.
#[no_mangle]
pub unsafe extern "C" fn elephc_tls_openssl_write(
    handle_id: i64,
    buffer: *const u8,
    length: usize,
) -> isize {
    if buffer.is_null() || length == 0 {
        return 0;
    }
    catch_unwind(AssertUnwindSafe(|| {
        let mut guard = lock_handles();
        let Some(entry) = guard.get_mut(&handle_id) else {
            return -1;
        };
        let ssl = entry.ssl as *mut SSL;
        let length = length.min(c_int::MAX as usize) as c_int;
        loop {
            let written = SSL_write(ssl, buffer as *const c_void, length);
            if written > 0 {
                return written as isize;
            }
            match SSL_get_error(ssl, written) {
                SSL_ERROR_WANT_READ | SSL_ERROR_WANT_WRITE => continue,
                _ => {
                    record_ssl_error("TLS write failed");
                    return -1;
                }
            }
        }
    }))
    .unwrap_or(-1)
}

/// C ABI: writes a memory BIO's contents to this thread's session stash.
fn stash_session(bytes: Vec<u8>) -> *const c_char {
    let owned = CString::new(bytes).unwrap_or_default();
    SESSION_PEM
        .try_with(|slot| {
            *slot.borrow_mut() = owned;
            slot.borrow().as_ptr()
        })
        .unwrap_or(ptr::null())
}

/// C ABI: exports the current TLS session as PEM, or an empty string.
#[no_mangle]
pub extern "C" fn elephc_tls_openssl_export_session(handle_id: i64) -> *const c_char {
    catch_unwind(AssertUnwindSafe(|| unsafe {
        let mut guard = lock_handles();
        let Some(entry) = guard.get_mut(&handle_id) else {
            return ptr::null();
        };
        let session = SSL_get_session(entry.ssl as *const SSL);
        if session.is_null() {
            return ptr::null();
        }
        let bio = BIO_new(BIO_s_mem());
        if bio.is_null() {
            return ptr::null();
        }
        if PEM_write_bio_SSL_SESSION(bio, session) != 1 {
            BIO_free(bio);
            return ptr::null();
        }
        let mut data: *mut c_char = ptr::null_mut();
        let length = BIO_ctrl(
            bio,
            BIO_CTRL_INFO,
            0,
            &mut data as *mut *mut c_char as *mut c_void,
        );
        let bytes = if length > 0 && !data.is_null() {
            std::slice::from_raw_parts(data as *const u8, length as usize).to_vec()
        } else {
            Vec::new()
        };
        BIO_free(bio);
        stash_session(bytes)
    }))
    .unwrap_or(ptr::null())
}

/// C ABI: returns whether the current connection resumed a session.
#[no_mangle]
pub extern "C" fn elephc_tls_openssl_session_reused(handle_id: i64) -> i64 {
    catch_unwind(AssertUnwindSafe(|| {
        let guard = lock_handles();
        guard
            .get(&handle_id)
            .map(|entry| unsafe { SSL_session_reused(entry.ssl as *const SSL) as i64 })
            .unwrap_or(-1)
    }))
    .unwrap_or(-1)
}

/// C ABI: reports the TLS 1.3 early-data status of a live handle.
///
/// `0` = not sent, `1` = rejected, `2` = accepted; `-1` for an unknown handle.
#[no_mangle]
pub extern "C" fn elephc_tls_openssl_early_data_status(handle_id: i64) -> i64 {
    catch_unwind(AssertUnwindSafe(|| {
        lock_handles()
            .get(&handle_id)
            .map(|entry| entry.early_data_status)
            .unwrap_or(-1)
    }))
    .unwrap_or(-1)
}

/// C ABI: sends close_notify and releases the connection handle.
#[no_mangle]
pub extern "C" fn elephc_tls_openssl_close(handle_id: i64) {
    let _ = catch_unwind(AssertUnwindSafe(|| {
        let Some(entry) = lock_handles().remove(&handle_id) else {
            return;
        };
        unsafe {
            SSL_shutdown(entry.ssl as *mut SSL);
            SSL_free(entry.ssl as *mut SSL);
            SSL_CTX_free(entry.ctx as *mut SSL_CTX);
        }
        drop(entry.stream);
    }));
}

/// C ABI: returns the thread-local last error, never NULL.
#[no_mangle]
pub extern "C" fn elephc_tls_openssl_last_error() -> *const c_char {
    LAST_ERROR
        .try_with(|slot| slot.borrow().as_ptr())
        .unwrap_or(ptr::null())
}

/// C ABI: clears the thread-local last error.
#[no_mangle]
pub extern "C" fn elephc_tls_openssl_clear_error() {
    clear_error();
}

// ---------------------------------------------------------------------------
// Drop-in elephc-tls ABI aliases
// ---------------------------------------------------------------------------

/// C ABI: secure client connect with default trust roots.
#[no_mangle]
pub unsafe extern "C" fn elephc_tls_connect(
    host: *const u8,
    host_len: usize,
    port: u16,
) -> i64 {
    elephc_tls_openssl_connect(host, host_len, port)
}

/// C ABI: insecure client connect.
#[no_mangle]
pub unsafe extern "C" fn elephc_tls_connect_insecure(
    host: *const u8,
    host_len: usize,
    port: u16,
) -> i64 {
    elephc_tls_openssl_connect_insecure(host, host_len, port)
}

/// C ABI: client connect with a custom CA bundle.
#[no_mangle]
pub unsafe extern "C" fn elephc_tls_connect_cafile(
    host: *const u8,
    host_len: usize,
    port: u16,
    cafile: *const u8,
    cafile_len: usize,
) -> i64 {
    elephc_tls_openssl_connect_cafile(host, host_len, port, cafile, cafile_len)
}

/// C ABI: unsupported CA-directory variant for this backend.
#[no_mangle]
pub unsafe extern "C" fn elephc_tls_connect_capath(
    _host: *const u8,
    _host_len: usize,
    _port: u16,
    _capath: *const u8,
    _capath_len: usize,
) -> i64 {
    record_error("TLS capath is not supported by the libssl session backend");
    -1
}

/// C ABI: unsupported peer-name override for this backend.
#[no_mangle]
pub unsafe extern "C" fn elephc_tls_connect_peer_name(
    _host: *const u8,
    _host_len: usize,
    _port: u16,
    _peer_name: *const u8,
    _peer_name_len: usize,
) -> i64 {
    record_error("TLS peer_name is not supported by the libssl session backend");
    -1
}

/// C ABI: unsupported client-certificate variant for this backend.
#[no_mangle]
pub unsafe extern "C" fn elephc_tls_connect_client_cert(
    _host: *const u8,
    _host_len: usize,
    _port: u16,
    _cert: *const u8,
    _cert_len: usize,
    _key: *const u8,
    _key_len: usize,
) -> i64 {
    record_error("TLS client certificates are not supported by the libssl session backend");
    -1
}

/// C ABI: imports a PEM session and connects securely.
#[no_mangle]
pub unsafe extern "C" fn elephc_tls_connect_session(
    host: *const u8,
    host_len: usize,
    port: u16,
    session: *const u8,
    session_len: usize,
    early_ptr: *const u8,
    early_len: usize,
) -> i64 {
    elephc_tls_openssl_connect_session(
        host,
        host_len,
        port,
        session,
        session_len,
        early_ptr,
        early_len,
    )
}

/// C ABI: imports a PEM session and connects without certificate checks.
#[no_mangle]
pub unsafe extern "C" fn elephc_tls_connect_session_insecure(
    host: *const u8,
    host_len: usize,
    port: u16,
    session: *const u8,
    session_len: usize,
    early_ptr: *const u8,
    early_len: usize,
) -> i64 {
    elephc_tls_openssl_connect_session_insecure(
        host,
        host_len,
        port,
        session,
        session_len,
        early_ptr,
        early_len,
    )
}

/// C ABI: connects with an external PSK supplied by a PHP callback.
#[no_mangle]
pub unsafe extern "C" fn elephc_tls_connect_psk(
    host: *const u8,
    host_len: usize,
    port: u16,
    descriptor: *mut c_void,
    adapter: *const c_void,
    insecure: c_int,
    early_ptr: *const u8,
    early_len: usize,
) -> i64 {
    elephc_tls_openssl_connect_psk(
        host,
        host_len,
        port,
        descriptor,
        adapter,
        insecure,
        early_ptr,
        early_len,
    )
}

/// C ABI: reports the TLS 1.3 early-data status of a live handle.
#[no_mangle]
pub extern "C" fn elephc_tls_early_data_status(handle_id: i64) -> i64 {
    elephc_tls_openssl_early_data_status(handle_id)
}

/// C ABI: reads plaintext from a libssl handle.
#[no_mangle]
pub unsafe extern "C" fn elephc_tls_read(
    handle_id: i64,
    buffer: *mut u8,
    length: usize,
) -> isize {
    elephc_tls_openssl_read(handle_id, buffer, length)
}

/// C ABI: writes plaintext to a libssl handle.
#[no_mangle]
pub unsafe extern "C" fn elephc_tls_write(
    handle_id: i64,
    buffer: *const u8,
    length: usize,
) -> isize {
    elephc_tls_openssl_write(handle_id, buffer, length)
}

/// C ABI: closes a libssl handle.
#[no_mangle]
pub extern "C" fn elephc_tls_close(handle_id: i64) {
    elephc_tls_openssl_close(handle_id);
}

/// C ABI: returns the thread-local last error.
#[no_mangle]
pub extern "C" fn elephc_tls_last_error() -> *const c_char {
    elephc_tls_openssl_last_error()
}

/// C ABI: clears the thread-local last error.
#[no_mangle]
pub extern "C" fn elephc_tls_clear_error() {
    elephc_tls_openssl_clear_error();
}

/// C ABI: unsupported fd-attach variant for this backend.
#[no_mangle]
pub unsafe extern "C" fn elephc_tls_attach_fd(_fd: i32, _host: *const u8, _host_len: usize) -> i64 {
    record_error("stream_socket_enable_crypto is not supported by the libssl session backend");
    -1
}

/// C ABI: unsupported fd-attach with client certificate.
#[no_mangle]
pub unsafe extern "C" fn elephc_tls_attach_fd_client_cert(
    _fd: i32,
    _host: *const u8,
    _host_len: usize,
    _cert: *const u8,
    _cert_len: usize,
    _key: *const u8,
    _key_len: usize,
) -> i64 {
    record_error("stream_socket_enable_crypto is not supported by the libssl session backend");
    -1
}

#[cfg(all(test, elephc_tls_openssl_native))]
mod tests {
    use super::*;
    use std::net::TcpListener;
    use std::os::fd::AsRawFd;
    use std::os::unix::ffi::OsStrExt;
    use std::path::PathBuf;
    use std::sync::mpsc;
    use std::thread;

    /// Throwaway self-signed certificate generated for the elephc test suite.
    const TEST_CERT_PEM: &str = "\
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

    /// The matching unencrypted PKCS#8 private key.
    const TEST_KEY_PEM: &str = "\
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

    /// Writes the test certificate and key into a unique temporary directory.
    fn certificate_files() -> (PathBuf, PathBuf) {
        let dir = std::env::temp_dir().join(format!(
            "elephc-tls-openssl-{}-{:?}",
            std::process::id(),
            thread::current().id()
        ));
        std::fs::create_dir_all(&dir).expect("create certificate directory");
        let cert = dir.join("cert.pem");
        let key = dir.join("key.pem");
        std::fs::write(&cert, TEST_CERT_PEM).expect("write certificate");
        std::fs::write(&key, TEST_KEY_PEM).expect("write private key");
        (cert, key)
    }

    /// Creates a TLS 1.2 server context with a server-session cache.
    unsafe fn server_context(cert: &PathBuf, key: &PathBuf) -> *mut SSL_CTX {
        let ctx = SSL_CTX_new(TLS_server_method());
        assert!(!ctx.is_null());
        let cert = CString::new(cert.as_os_str().as_bytes()).unwrap();
        let key = CString::new(key.as_os_str().as_bytes()).unwrap();
        assert_eq!(
            SSL_CTX_use_certificate_file(ctx, cert.as_ptr(), SSL_FILETYPE_PEM),
            1
        );
        assert_eq!(
            SSL_CTX_use_PrivateKey_file(ctx, key.as_ptr(), SSL_FILETYPE_PEM),
            1
        );
        SSL_CTX_set_verify(ctx, SSL_VERIFY_NONE, None);
        assert_eq!(
            SSL_CTX_set_session_id_context(ctx, b"elephc".as_ptr(), 6),
            1
        );
        let _ = SSL_CTX_ctrl(
            ctx,
            SSL_CTRL_SET_SESS_CACHE_MODE,
            SSL_SESS_CACHE_SERVER,
            ptr::null_mut(),
        );
        assert_eq!(
            SSL_CTX_ctrl(
                ctx,
                SSL_CTRL_SET_MAX_PROTO_VERSION,
                TLS1_2_VERSION,
                ptr::null_mut()
            ),
            1
        );
        ctx
    }

    /// Runs the two-connection TLS 1.2 server and reports session reuse per connection.
    fn serve_two(cert: PathBuf, key: PathBuf, port_tx: mpsc::Sender<u16>, reuse_tx: mpsc::Sender<Vec<i64>>) {
        let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind test server");
        port_tx
            .send(listener.local_addr().expect("server address").port())
            .expect("send server port");
        let mut reused = Vec::new();
        unsafe {
            let ctx = server_context(&cert, &key);
            for index in 0..2 {
                let (stream, _) = listener.accept().expect("accept TLS client");
                let ssl = SSL_new(ctx);
                assert!(!ssl.is_null());
                assert_eq!(SSL_set_fd(ssl, stream.as_raw_fd()), 1);
                assert_eq!(SSL_accept(ssl), 1);
                reused.push(SSL_session_reused(ssl) as i64);
                let mut byte = [0u8; 1];
                assert_eq!(SSL_read(ssl, byte.as_mut_ptr() as *mut c_void, 1), 1);
                let reply = if index == 0 { b"one" } else { b"two" };
                assert_eq!(
                    SSL_write(ssl, reply.as_ptr() as *const c_void, reply.len() as c_int),
                    reply.len() as c_int
                );
                SSL_shutdown(ssl);
                SSL_free(ssl);
                drop(stream);
            }
            SSL_CTX_free(ctx);
        }
        reuse_tx.send(reused).expect("send reuse report");
    }

    /// Reads exactly `expected` bytes from a TLS handle.
    fn read_exact_tls(handle: i64, expected: usize) -> Vec<u8> {
        let mut output = Vec::with_capacity(expected);
        while output.len() < expected {
            let mut chunk = [0u8; 16];
            let read = unsafe {
                elephc_tls_openssl_read(handle, chunk.as_mut_ptr(), chunk.len())
            };
            assert!(read > 0, "TLS read failed: {:?}", last_error());
            output.extend_from_slice(&chunk[..read as usize]);
        }
        output
    }

    /// Writes one byte to a TLS handle.
    fn write_byte_tls(handle: i64, byte: u8) {
        let written = unsafe { elephc_tls_openssl_write(handle, &byte, 1) };
        assert_eq!(written, 1, "TLS write failed: {:?}", last_error());
    }

    /// Reads the thread-local last error as a string.
    fn last_error() -> String {
        unsafe {
            CStr::from_ptr(elephc_tls_openssl_last_error())
                .to_string_lossy()
                .into_owned()
        }
    }

    /// Connects insecure over TLS 1.2, optionally installing an imported session.
    fn connect_insecure_tls12(host: &str, port: u16, session: Option<&CStr>) -> i64 {
        unsafe {
            let host_ptr = host.as_ptr();
            match session {
                Some(session) => {
                    let session = session.to_bytes();
                    let ctx = create_client_ctx(true, None, None, Some(TLS1_2_VERSION));
                    let Some(ctx) = ctx else { return -1; };
                    let ssl = SSL_new(ctx);
                    assert!(!ssl.is_null());
                    let stream = TcpStream::connect((host, port)).expect("connect client");
                    assert_eq!(SSL_set_fd(ssl, stream.as_raw_fd()), 1);
                    let session = parse_session(session).expect("parse imported session");
                    assert_eq!(SSL_set_session(ssl, session), 1);
                    SSL_SESSION_free(session);
                    assert_eq!(SSL_connect(ssl), 1, "client handshake: {:?}", last_error());
                    let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
                    lock_handles().insert(
                        id,
                        Entry {
                            ssl: ssl as usize,
                            ctx: ctx as usize,
                            stream,
                            early_data_status: 0,
                        },
                    );
                    let _ = host_ptr;
                    id
                }
                None => {
                    let ctx = create_client_ctx(true, None, None, Some(TLS1_2_VERSION));
                    let Some(ctx) = ctx else { return -1; };
                    let ssl = SSL_new(ctx);
                    assert!(!ssl.is_null());
                    let stream = TcpStream::connect((host, port)).expect("connect client");
                    assert_eq!(SSL_set_fd(ssl, stream.as_raw_fd()), 1);
                    assert_eq!(SSL_connect(ssl), 1, "client handshake: {:?}", last_error());
                    let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
                    lock_handles().insert(
                        id,
                        Entry {
                            ssl: ssl as usize,
                            ctx: ctx as usize,
                            stream,
                            early_data_status: 0,
                        },
                    );
                    let _ = host_ptr;
                    id
                }
            }
        }
    }

    /// The PSK the loopback server accepts.
    static SERVER_PSK: OnceLock<Vec<u8>> = OnceLock::new();

    /// Early data the loopback server advertises and accepts.
    const SERVER_MAX_EARLY_DATA: u32 = 1024;

    /// TLS 1.3 server callback: accepts the `client` identity and returns the
    /// session carrying the configured external PSK.
    unsafe extern "C" fn server_psk_find_session_cb(
        ssl: *mut SSL,
        identity: *const u8,
        identity_len: usize,
        session_out: *mut *mut SSL_SESSION,
    ) -> c_int {
        let identity = std::slice::from_raw_parts(identity, identity_len);
        if identity != b"client" {
            return 0;
        }
        let Some(psk) = SERVER_PSK.get() else {
            return 0;
        };
        let Some(session) = build_psk_session(ssl, psk, ptr::null()) else {
            return 0;
        };
        if SSL_SESSION_set_max_early_data(session, SERVER_MAX_EARLY_DATA) != 1 {
            SSL_SESSION_free(session);
            return 0;
        }
        *session_out = session;
        1
    }

    /// Creates a certificate-less TLS 1.3 server context that only accepts the
    /// external PSK from `server_psk_find_session_cb`.
    unsafe fn server_context_psk(psk: &[u8]) -> *mut SSL_CTX {
        let _ = SERVER_PSK.set(psk.to_vec());
        let ctx = SSL_CTX_new(TLS_server_method());
        assert!(!ctx.is_null());
        SSL_CTX_set_verify(ctx, SSL_VERIFY_NONE, None);
        SSL_CTX_set_psk_find_session_callback(ctx, Some(server_psk_find_session_cb));
        assert_eq!(
            SSL_CTX_set_ciphersuites(ctx, c"TLS_AES_128_GCM_SHA256".as_ptr()),
            1
        );
        assert_eq!(
            SSL_CTX_set_max_early_data(ctx, SERVER_MAX_EARLY_DATA),
            1
        );
        assert_eq!(
            SSL_CTX_set_recv_max_early_data(ctx, SERVER_MAX_EARLY_DATA),
            1
        );
        assert_eq!(
            SSL_CTX_ctrl(
                ctx,
                SSL_CTRL_SET_MAX_PROTO_VERSION,
                TLS1_3_VERSION as c_long,
                ptr::null_mut(),
            ),
            1
        );
        ctx
    }

    /// Runs a one-connection TLS 1.3 PSK server and reports whether it accepted
    /// plus the early data it read.
    fn serve_psk(
        port_tx: mpsc::Sender<u16>,
        done_tx: mpsc::Sender<(bool, Vec<u8>)>,
        psk: Vec<u8>,
    ) {
        let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind PSK server");
        port_tx
            .send(listener.local_addr().expect("PSK server address").port())
            .expect("send PSK server port");
        let result = unsafe {
            let ctx = server_context_psk(&psk);
            let (stream, _) = listener.accept().expect("accept PSK client");
            let ssl = SSL_new(ctx);
            assert!(!ssl.is_null());
            assert_eq!(SSL_set_fd(ssl, stream.as_raw_fd()), 1);
            let mut early = Vec::new();
            let mut buffer = [0u8; SERVER_MAX_EARLY_DATA as usize];
            loop {
                let mut readbytes = 0usize;
                let state = SSL_read_early_data(
                    ssl,
                    buffer.as_mut_ptr() as *mut c_void,
                    buffer.len(),
                    &mut readbytes,
                );
                if state == 1 {
                    early.extend_from_slice(&buffer[..readbytes]);
                    continue;
                }
                if state == 2 {
                    break;
                }
                eprintln!("PSK server early-data read failed: {:?}", last_error());
                break;
            }
            let handshake = SSL_accept(ssl) == 1;
            if !handshake {
                eprintln!("PSK server handshake failed: {:?}", last_error());
            }
            if handshake {
                let mut byte = [0u8; 1];
                assert_eq!(SSL_read(ssl, byte.as_mut_ptr() as *mut c_void, 1), 1);
                assert_eq!(
                    SSL_write(ssl, b"psk".as_ptr() as *const c_void, 3),
                    3
                );
            }
            SSL_shutdown(ssl);
            SSL_free(ssl);
            SSL_CTX_free(ctx);
            (handshake, early)
        };
        done_tx.send(result).expect("send PSK server result");
    }

    /// Test adapter: returns `identity_len | identity | psk` exactly as the PHP
    /// wrapper does, without needing a compiled PHP program.
    unsafe extern "C" fn psk_test_adapter(
        _descriptor: *mut c_void,
        _object: *mut c_void,
        spec: *mut PskCallSpec,
    ) -> i64 {
        let encoded: &[u8] = b"\x00\x00\x00\x06clientsecret";
        if encoded.len() > (*spec).out_cap as usize {
            (*spec).status = -1;
            return 0;
        }
        ptr::copy_nonoverlapping(encoded.as_ptr(), (*spec).out_buf, encoded.len());
        (*spec).out_len = encoded.len() as i64;
        (*spec).status = 0;
        0
    }

    /// Verifies the TLS 1.3 external-PSK connect path against a loopback server.
    #[test]
    fn tls13_external_psk_round_trip() {
        let (port_tx, port_rx) = mpsc::channel();
        let (done_tx, done_rx) = mpsc::channel();
        let server = thread::spawn(move || serve_psk(port_tx, done_tx, b"secret".to_vec()));
        let port = port_rx.recv().expect("PSK server port");
        let early: &[u8] = b"early request";

        let handle = unsafe {
            elephc_tls_openssl_connect_psk(
                b"127.0.0.1".as_ptr(),
                9,
                port,
                1 as *mut c_void,
                psk_test_adapter as *const c_void,
                1,
                early.as_ptr(),
                early.len(),
            )
        };
        assert!(handle > 0, "PSK connect: {:?}", last_error());
        assert_eq!(elephc_tls_openssl_early_data_status(handle), 2);
        assert_eq!(
            unsafe { elephc_tls_openssl_write(handle, b"a".as_ptr(), 1) },
            1
        );
        assert_eq!(read_exact_tls(handle, 3), b"psk");
        elephc_tls_openssl_close(handle);

        let (accepted, received) = done_rx.recv().expect("PSK server result");
        assert!(accepted, "server handshake failed");
        assert_eq!(received, early);
        server.join().expect("PSK server thread");
    }

    /// A TLS 1.3 server context with a session cache for ticket delivery.
    unsafe fn server_context_tls13(cert: &PathBuf, key: &PathBuf) -> *mut SSL_CTX {
        let ctx = SSL_CTX_new(TLS_server_method());
        assert!(!ctx.is_null());
        let cert = CString::new(cert.as_os_str().as_bytes()).unwrap();
        let key = CString::new(key.as_os_str().as_bytes()).unwrap();
        assert_eq!(
            SSL_CTX_use_certificate_file(ctx, cert.as_ptr(), SSL_FILETYPE_PEM),
            1
        );
        assert_eq!(
            SSL_CTX_use_PrivateKey_file(ctx, key.as_ptr(), SSL_FILETYPE_PEM),
            1
        );
        SSL_CTX_set_verify(ctx, SSL_VERIFY_NONE, None);
        assert_eq!(
            SSL_CTX_set_session_id_context(ctx, b"elephc".as_ptr(), 6),
            1
        );
        let _ = SSL_CTX_ctrl(
            ctx,
            SSL_CTRL_SET_SESS_CACHE_MODE,
            SSL_SESS_CACHE_SERVER,
            ptr::null_mut(),
        );
        assert_eq!(
            SSL_CTX_ctrl(
                ctx,
                SSL_CTRL_SET_MAX_PROTO_VERSION,
                TLS1_3_VERSION as c_long,
                ptr::null_mut(),
            ),
            1
        );
        ctx
    }

    /// Runs a one-connection TLS 1.3 server that issues a session ticket.
    fn serve_tls13(
        cert: PathBuf,
        key: PathBuf,
        port_tx: mpsc::Sender<u16>,
        done_tx: mpsc::Sender<bool>,
    ) {
        let listener = TcpListener::bind(("127.0.0.1", 0)).expect("bind TLS 1.3 server");
        port_tx
            .send(listener.local_addr().expect("TLS 1.3 server address").port())
            .expect("send TLS 1.3 server port");
        let accepted = unsafe {
            let ctx = server_context_tls13(&cert, &key);
            let (stream, _) = listener.accept().expect("accept TLS 1.3 client");
            let ssl = SSL_new(ctx);
            assert!(!ssl.is_null());
            assert_eq!(SSL_set_fd(ssl, stream.as_raw_fd()), 1);
            let handshake = SSL_accept(ssl) == 1;
            if handshake {
                let mut byte = [0u8; 1];
                assert_eq!(SSL_read(ssl, byte.as_mut_ptr() as *mut c_void, 1), 1);
                assert_eq!(
                    SSL_write(ssl, b"tls13".as_ptr() as *const c_void, 5),
                    5
                );
            }
            SSL_shutdown(ssl);
            SSL_free(ssl);
            SSL_CTX_free(ctx);
            handshake
        };
        done_tx.send(accepted).expect("send TLS 1.3 server result");
    }

    /// Connects insecure over TLS 1.3 and publishes the handle.
    unsafe fn connect_insecure_tls13(host: &str, port: u16) -> i64 {
        let ctx = create_client_ctx(true, None, None, Some(TLS1_3_VERSION as c_long));
        let Some(ctx) = ctx else { return -1; };
        let ssl = SSL_new(ctx);
        assert!(!ssl.is_null());
        let stream = TcpStream::connect((host, port)).expect("connect TLS 1.3 client");
        assert_eq!(SSL_set_fd(ssl, stream.as_raw_fd()), 1);
        assert_eq!(SSL_connect(ssl), 1, "client TLS 1.3 handshake: {:?}", last_error());
        let id = NEXT_ID.fetch_add(1, Ordering::Relaxed);
        lock_handles().insert(
            id,
            Entry {
                ssl: ssl as usize,
                ctx: ctx as usize,
                stream,
                early_data_status: 0,
            },
        );
        id
    }

    /// Captured session PEMs from the test `session_new_cb` adapter.
    static SESSION_PEMS: OnceLock<Mutex<Vec<String>>> = OnceLock::new();

    /// Test adapter: records the PEM string argument of `session_new_cb`.
    unsafe extern "C" fn session_new_test_adapter(
        _descriptor: *mut c_void,
        _object: *mut c_void,
        spec: *mut PskCallSpec,
    ) -> i64 {
        if (*spec).argc != 1 {
            return 0;
        }
        let argument = &*(*spec).argv;
        if argument.tag != 1 || argument.hi <= 0 {
            return 0;
        }
        let bytes = std::slice::from_raw_parts(
            argument.lo as *const u8,
            argument.hi as usize,
        );
        let pem = String::from_utf8_lossy(bytes).into_owned();
        SESSION_PEMS
            .get_or_init(|| Mutex::new(Vec::new()))
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .push(pem);
        0
    }

    /// Verifies that libssl's new-session callback reaches compiled PHP with a
    /// PEM `SSL_SESSION` on a TLS 1.3 connection.
    #[test]
    fn tls13_session_new_callback_round_trip() {
        elephc_tls_set_session_new_cb(
            1 as *mut c_void,
            session_new_test_adapter as *const c_void,
        );
        let (cert, key) = certificate_files();
        let (port_tx, port_rx) = mpsc::channel();
        let (done_tx, done_rx) = mpsc::channel();
        let server = thread::spawn(move || serve_tls13(cert, key, port_tx, done_tx));
        let port = port_rx.recv().expect("TLS 1.3 server port");

        let handle = unsafe { connect_insecure_tls13("127.0.0.1", port) };
        assert!(handle > 0, "TLS 1.3 connect: {:?}", last_error());
        write_byte_tls(handle, b'a');
        assert_eq!(read_exact_tls(handle, 5), b"tls13");
        elephc_tls_openssl_close(handle);
        assert!(done_rx.recv().expect("TLS 1.3 server result"));
        server.join().expect("TLS 1.3 server thread");

        let mut captured = Vec::new();
        for _ in 0..40 {
            captured = SESSION_PEMS
                .get_or_init(|| Mutex::new(Vec::new()))
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .clone();
            if !captured.is_empty() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(25));
        }
        elephc_tls_set_session_new_cb(ptr::null_mut(), ptr::null());
        assert!(
            !captured.is_empty(),
            "session_new_cb was not invoked"
        );
        assert!(
            captured[0].contains("BEGIN SSL SESSION PARAMETERS"),
            "callback did not receive a PEM session: {captured:?}"
        );
    }

    /// Verifies that an exported TLS 1.2 session resumes on the next connection.
    /// Verifies that an exported TLS 1.2 session resumes on the next connection.
    #[test]
    fn tls12_session_resumption_round_trip() {
        let (cert, key) = certificate_files();
        let (port_tx, port_rx) = mpsc::channel();
        let (reuse_tx, reuse_rx) = mpsc::channel();
        let server = thread::spawn(move || serve_two(cert, key, port_tx, reuse_tx));
        let port = port_rx.recv().expect("server port");

        let first = connect_insecure_tls12("127.0.0.1", port, None);
        assert!(first > 0, "first connect: {:?}", last_error());
        write_byte_tls(first, b'a');
        assert_eq!(read_exact_tls(first, 3), b"one");
        let session = {
            let pointer = elephc_tls_openssl_export_session(first);
            assert!(!pointer.is_null(), "session export failed");
            unsafe { CStr::from_ptr(pointer).to_owned() }
        };
        assert!(session.to_bytes().starts_with(b"-----BEGIN SSL SESSION PARAMETERS-----"));
        elephc_tls_openssl_close(first);

        let second = connect_insecure_tls12("127.0.0.1", port, Some(&session));
        assert!(second > 0, "resumed connect: {:?}", last_error());
        assert_eq!(elephc_tls_openssl_session_reused(second), 1);
        write_byte_tls(second, b'b');
        assert_eq!(read_exact_tls(second, 3), b"two");
        elephc_tls_openssl_close(second);

        let reused = reuse_rx.recv().expect("server reuse report");
        server.join().expect("server thread");
        assert_eq!(reused, vec![0, 1]);
    }
}
