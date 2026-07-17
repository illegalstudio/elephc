//! Purpose:
//! Multi-driver database bridge for the elephc PDO implementation. Exposes a
//! small, stable, driver-agnostic C ABI (`elephc_pdo_*`) that the elephc PDO
//! prelude calls through `extern "elephc_pdo"` declarations; each call dispatches
//! to a registered PDO driver based on the handle's driver, selected from the
//! DSN prefix at `open()`.
//!
//! Called from:
//! - Compiled PHP programs that use PDO, via the elephc-PHP prelude's `extern`
//!   declarations (`src/pdo_prelude.rs`). The symbols are only referenced by
//!   PDO-using programs, so non-PDO binaries never link `-lelephc_pdo`.
//! - Rust unit tests in this crate (`cargo test -p elephc-pdo`).
//!
//! Key details:
//! - Two global handle tables index live connections / statements by `i64` IDs,
//!   each wrapped in a driver-tagged enum (`Conn`, `Stmt`). A small persistent
//!   DSN pool can keep selected connections open for process-local reuse. The C
//!   ABI never exposes raw pointers. elephc programs are effectively
//!   single-threaded, so the table mutexes are simplicity, not contention
//!   management.
//! - Fallible entry points collapse failure to a `-1`/`0` sentinel. String
//!   results return `*const c_char` into a per-result static buffer that elephc
//!   copies into an owned PHP string immediately on return.
//! - Every `extern "C"` entry point runs its body inside `ffi_guard`, a
//!   `catch_unwind` panic firewall (F-QUAL-02), and takes every table lock through
//!   `lock_recover`. Without the pair, one panic under a `conns()`/`stmts()` lock
//!   would poison that mutex and brick PDO for the whole process, and the unwind
//!   out of a plain `extern "C"` function would abort the compiled program instead
//!   of surfacing a catchable `PDOException`.
//! - The default drivers are bundled (SQLite) / pure-Rust (PostgreSQL,
//!   MySQL/MariaDB), so their binaries have no system database-client runtime
//!   dependency. The optional PDO_DBLIB profile links FreeTDS like php-src;
//!   PDO_FIREBIRD uses the pure-Rust Firebird wire protocol on every target,
//!   PDO_ODBC links the system driver manager, and PDO_OCI loads Oracle Instant
//!   Client dynamically through ODPI-C.

mod driver;
#[cfg(feature = "dblib")]
mod dblib;
#[cfg(feature = "firebird")]
mod firebird;
mod ini;
mod my;
#[cfg(any(feature = "odbc", feature = "informix"))]
mod odbc;
#[cfg(feature = "oci")]
mod oci;
#[path = "pg.rs"]
#[cfg_attr(feature = "libpq-gss", allow(dead_code))]
mod pg_native;
#[cfg(feature = "libpq-gss")]
#[path = "pg_libpq.rs"]
mod pg;
#[cfg(not(feature = "libpq-gss"))]
use pg_native as pg;
mod sqlite;

use std::collections::{HashMap, HashSet};
use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_void};
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Mutex, MutexGuard, OnceLock};

/// A live connection, tagged by its driver.
enum Conn {
    #[cfg(feature = "dblib")]
    Dblib(dblib::DblibConn),
    #[cfg(feature = "firebird")]
    Firebird(firebird::FirebirdConn),
    #[cfg(any(feature = "odbc", feature = "informix"))]
    Odbc(odbc::OdbcConn),
    #[cfg(feature = "oci")]
    Oci(oci::OciConn),
    Sqlite(sqlite::SqliteConn),
    Postgres(pg::PgConn),
    Mysql(my::MyConn),
}

impl Conn {
    /// Returns the central registry identity for this live connection.
    fn driver_kind(&self) -> driver::DriverKind {
        match self {
            #[cfg(feature = "dblib")]
            Self::Dblib(_) => driver::DriverKind::Dblib,
            #[cfg(feature = "firebird")]
            Self::Firebird(_) => driver::DriverKind::Firebird,
            #[cfg(any(feature = "odbc", feature = "informix"))]
            Self::Odbc(connection) => connection.driver_kind(),
            #[cfg(feature = "oci")]
            Self::Oci(_) => driver::DriverKind::Oci,
            Self::Sqlite(_) => driver::DriverKind::Sqlite,
            Self::Postgres(_) => driver::DriverKind::Pgsql,
            Self::Mysql(_) => driver::DriverKind::Mysql,
        }
    }
}

/// A live prepared statement, tagged by its driver.
enum Stmt {
    #[cfg(feature = "dblib")]
    Dblib(dblib::DblibStmt),
    #[cfg(feature = "firebird")]
    Firebird(firebird::FirebirdStmt),
    #[cfg(any(feature = "odbc", feature = "informix"))]
    Odbc(odbc::OdbcStmt),
    #[cfg(feature = "oci")]
    Oci(oci::OciStmt),
    Sqlite(sqlite::SqliteStmt),
    Postgres(pg::PgStmt),
    Mysql(my::MyStmt),
}

/// Global connection table, keyed by the `i64` IDs handed back to the caller.
fn conns() -> &'static Mutex<HashMap<i64, Conn>> {
    static CONNS: OnceLock<Mutex<HashMap<i64, Conn>>> = OnceLock::new();
    CONNS.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Global prepared-statement table, keyed by the `i64` IDs handed back.
fn stmts() -> &'static Mutex<HashMap<i64, Stmt>> {
    static STMTS: OnceLock<Mutex<HashMap<i64, Stmt>>> = OnceLock::new();
    STMTS.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Process-local persistent connection pool, keyed by the pair of the fully
/// materialized DSN passed into the bridge (after constructor credentials have been
/// folded in) and the caller's `PDO::ATTR_PERSISTENT` key string.
///
/// The key string is the second half of the key because php-src's persistent
/// hashkey is built from the DSN *and* that string whenever `ATTR_PERSISTENT` was
/// given as a non-numeric, non-empty string (`pdo_dbh.c:389-404`) — so two
/// persistent connections to the SAME DSN under DIFFERENT key strings are distinct
/// pooled entries, and only a plain boolean-persistent open (key `""`, F-CORE-16)
/// pools by DSN alone. Keying on the DSN by itself, as this did, wrongly collapsed
/// two differently-named pools onto one shared connection.
fn persistent_conns() -> &'static Mutex<HashMap<(String, String), i64>> {
    static PERSISTENT_CONNS: OnceLock<Mutex<HashMap<(String, String), i64>>> = OnceLock::new();
    PERSISTENT_CONNS.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Set of connection handles owned by the persistent pool. Release decrements
/// ownership but leaves these handles open for later checkout.
fn persistent_ids() -> &'static Mutex<HashSet<i64>> {
    static PERSISTENT_IDS: OnceLock<Mutex<HashSet<i64>>> = OnceLock::new();
    PERSISTENT_IDS.get_or_init(|| Mutex::new(HashSet::new()))
}

/// Counts live PDO objects currently owning each pooled connection handle.
/// A zero count keeps the native session cached but makes it eligible for the
/// PHP 8.6 PostgreSQL disconnect-equivalent `DISCARD ALL` reset.
fn persistent_owner_counts() -> &'static Mutex<HashMap<i64, usize>> {
    static COUNTS: OnceLock<Mutex<HashMap<i64, usize>>> = OnceLock::new();
    COUNTS.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Serializes persistent checkout, liveness validation, eviction, and reconnect.
fn persistent_checkout_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

/// Returns a fresh, never-reused handle ID. IDs start at 1 so `0` and `-1`
/// remain available as "absent" / "error" sentinels.
fn next_id() -> i64 {
    static NEXT: AtomicI64 = AtomicI64::new(1);
    NEXT.fetch_add(1, Ordering::SeqCst)
}

/// Runs an FFI entry-point body, converting any panic into `fallback` so a panic
/// never unwinds across the C ABI boundary (F-QUAL-02). These entry points are plain
/// `extern "C"` (not `extern "C-unwind"`), so on rustc ≥ 1.81 an unwinding panic out
/// of one ABORTS the whole compiled PHP process — over an internal `unwrap`, a
/// debug-build overflow, or an unexpected panic from the `postgres`/`mysql` client
/// crates. Catching it here degrades the call into the same well-defined "failed"
/// answer the entry point's own docblock already promises for an unknown handle
/// (`-1`/`0`, the empty string, …), which the prelude turns into a catchable
/// `PDOException`. Mirrors the same pair in `elephc-image` / `elephc-phar`.
///
/// `AssertUnwindSafe` is sound here: the bodies touch only the process-global handle
/// tables, each guarded by its own `Mutex`, and a lock poisoned by a caught panic is
/// reclaimed by [`lock_recover`] rather than re-panicking — so a caught panic can
/// leave a table logically stale, never memory-unsafe.
///
/// `pub(crate)` (like `elephc-image`'s) so the driver modules can guard their own
/// externs too: `sqlite::elephc_pdo_udf_stash_bytes` is the one entry point outside
/// this file, and it is guarded as well — the firewall covers every `#[no_mangle]` body
/// in the crate without exception.
pub(crate) fn ffi_guard<T>(fallback: T, body: impl FnOnce() -> T) -> T {
    match std::panic::catch_unwind(std::panic::AssertUnwindSafe(body)) {
        Ok(value) => value,
        Err(_) => fallback,
    }
}

/// Locks a process-global table, recovering the guard if a previously caught panic
/// poisoned the mutex. This is the other half of the [`ffi_guard`] firewall: once a
/// panic escapes a body that held a `conns()`/`stmts()` lock, that mutex stays
/// poisoned forever, and a plain `.lock().unwrap()` would then panic on EVERY later
/// PDO call in the process — unrelated connections included — each of those panics
/// aborting across the C ABI. The payload is still structurally valid (the tables are
/// plain maps, the cells plain buffers), so reusing it lets the bridge keep serving.
/// `pub(crate)` for the same reason as [`ffi_guard`].
pub(crate) fn lock_recover<T>(m: &Mutex<T>) -> MutexGuard<'_, T> {
    m.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// Returns a pointer to a `'static` NUL-terminated C string literal. Used as the
/// [`ffi_guard`] fallback of the `*const c_char` entry points, which must hand back a
/// readable C string even in the panic path. It deliberately does NOT route through
/// the per-result static cells: the panic being caught may have happened while one of
/// those cells was locked or half-written, so the fallback must not depend on any
/// state the panicking body could have touched.
fn static_cstr(bytes: &'static [u8]) -> *const c_char {
    debug_assert_eq!(
        bytes.last(),
        Some(&0),
        "static_cstr needs a NUL-terminated literal"
    );
    bytes.as_ptr() as *const c_char
}

/// Static buffer holding the last message captured by a failed `elephc_pdo_open`.
fn open_error_cell() -> &'static Mutex<CString> {
    static C: OnceLock<Mutex<CString>> = OnceLock::new();
    C.get_or_init(|| Mutex::new(CString::default()))
}

/// Static SQLSTATE captured alongside the last failed connection open.
fn open_sqlstate_cell() -> &'static Mutex<CString> {
    static C: OnceLock<Mutex<CString>> = OnceLock::new();
    C.get_or_init(|| Mutex::new(CString::default()))
}

/// Native driver code captured alongside the last failed connection open.
fn open_native_code_cell() -> &'static AtomicI64 {
    static CODE: AtomicI64 = AtomicI64::new(0);
    &CODE
}

/// Stores a failed-open message and any driver-specific constructor diagnostic.
fn store_open_failure(dsn: &str, message: &str) {
    store_cstr(open_error_cell(), message);
    let (sqlstate, native_code) = if dsn.starts_with("oci:") {
        #[cfg(feature = "oci")]
        {
            let (state, code) = oci::open_diagnostic(message);
            (state.to_string(), code)
        }
        #[cfg(not(feature = "oci"))]
        {
            (String::new(), 0)
        }
    } else if dsn.starts_with("odbc:") || dsn.starts_with("informix:") {
        #[cfg(any(feature = "odbc", feature = "informix"))]
        {
            odbc::open_diagnostic()
        }
        #[cfg(not(any(feature = "odbc", feature = "informix")))]
        {
            (String::new(), 0)
        }
    } else {
        (String::new(), 0)
    };
    store_cstr(open_sqlstate_cell(), &sqlstate);
    open_native_code_cell().store(native_code, Ordering::Relaxed);
}

/// Static buffer for the most recent `elephc_pdo_errmsg` result.
fn errmsg_cell() -> &'static Mutex<CString> {
    static C: OnceLock<Mutex<CString>> = OnceLock::new();
    C.get_or_init(|| Mutex::new(CString::default()))
}

/// Static buffer for the most recent `elephc_pdo_column_name` result.
fn colname_cell() -> &'static Mutex<CString> {
    static C: OnceLock<Mutex<CString>> = OnceLock::new();
    C.get_or_init(|| Mutex::new(CString::default()))
}

/// Static buffer for the most recent `elephc_pdo_column_table_name` result.
fn table_name_cell() -> &'static Mutex<CString> {
    static C: OnceLock<Mutex<CString>> = OnceLock::new();
    C.get_or_init(|| Mutex::new(CString::default()))
}

/// Static buffer for the most recent `elephc_pdo_column_decltype` result.
fn decltype_cell() -> &'static Mutex<CString> {
    static C: OnceLock<Mutex<CString>> = OnceLock::new();
    C.get_or_init(|| Mutex::new(CString::default()))
}

/// Static buffer for the most recent `elephc_pdo_column_native_type` result.
fn native_type_cell() -> &'static Mutex<CString> {
    static C: OnceLock<Mutex<CString>> = OnceLock::new();
    C.get_or_init(|| Mutex::new(CString::default()))
}

/// Return buffer for PDO_DBLIB column-source metadata.
fn dblib_column_source_cell() -> &'static Mutex<CString> {
    static CELL: OnceLock<Mutex<CString>> = OnceLock::new();
    CELL.get_or_init(|| Mutex::new(CString::new("").unwrap()))
}

/// Static buffer for the most recent emulated statement SQL result.
fn stmt_sent_sql_cell() -> &'static Mutex<CString> {
    static C: OnceLock<Mutex<CString>> = OnceLock::new();
    C.get_or_init(|| Mutex::new(CString::default()))
}

/// Static byte buffer for the most recent `elephc_pdo_column_data_ptr` result.
fn coldata_cell() -> &'static Mutex<Vec<u8>> {
    static C: OnceLock<Mutex<Vec<u8>>> = OnceLock::new();
    C.get_or_init(|| Mutex::new(Vec::new()))
}

/// Static buffer for the most recent `elephc_pdo_driver_name` result.
fn drivername_cell() -> &'static Mutex<CString> {
    static C: OnceLock<Mutex<CString>> = OnceLock::new();
    C.get_or_init(|| Mutex::new(CString::default()))
}

/// Static buffer for the most recently resolved `pdo.dsn.*` INI alias.
fn dsn_alias_cell() -> &'static Mutex<CString> {
    static C: OnceLock<Mutex<CString>> = OnceLock::new();
    C.get_or_init(|| Mutex::new(CString::default()))
}

/// Static buffer for the most recent `elephc_pdo_sqlstate` result.
fn sqlstate_cell() -> &'static Mutex<CString> {
    static C: OnceLock<Mutex<CString>> = OnceLock::new();
    C.get_or_init(|| Mutex::new(CString::default()))
}

/// Static buffer for the most recent `elephc_pdo_stmt_sqlstate` result.
fn stmt_sqlstate_cell() -> &'static Mutex<CString> {
    static C: OnceLock<Mutex<CString>> = OnceLock::new();
    C.get_or_init(|| Mutex::new(CString::default()))
}

/// Static buffer for the most recent `elephc_pdo_stmt_errmsg` result.
fn stmt_errmsg_cell() -> &'static Mutex<CString> {
    static C: OnceLock<Mutex<CString>> = OnceLock::new();
    C.get_or_init(|| Mutex::new(CString::default()))
}

/// Return buffer for PDO_DBLIB connection operating-system diagnostics.
fn dblib_os_errmsg_cell() -> &'static Mutex<CString> {
    static CELL: OnceLock<Mutex<CString>> = OnceLock::new();
    CELL.get_or_init(|| Mutex::new(CString::new("").unwrap()))
}

/// Return buffer for PDO_DBLIB statement operating-system diagnostics.
fn dblib_stmt_os_errmsg_cell() -> &'static Mutex<CString> {
    static CELL: OnceLock<Mutex<CString>> = OnceLock::new();
    CELL.get_or_init(|| Mutex::new(CString::new("").unwrap()))
}

/// Shared return buffer for textual PDO_FIREBIRD attributes.
fn firebird_attribute_cell() -> &'static Mutex<CString> {
    static CELL: OnceLock<Mutex<CString>> = OnceLock::new();
    CELL.get_or_init(|| Mutex::new(CString::new("").unwrap()))
}

/// Static buffer for the most recent `elephc_pdo_server_version` result.
fn server_version_cell() -> &'static Mutex<CString> {
    static C: OnceLock<Mutex<CString>> = OnceLock::new();
    C.get_or_init(|| Mutex::new(CString::default()))
}

/// Static buffer for the most recent `elephc_pdo_client_version` result.
fn client_version_cell() -> &'static Mutex<CString> {
    static C: OnceLock<Mutex<CString>> = OnceLock::new();
    C.get_or_init(|| Mutex::new(CString::default()))
}

/// Static buffer for the most recent `elephc_pdo_server_info` result.
fn server_info_cell() -> &'static Mutex<CString> {
    static C: OnceLock<Mutex<CString>> = OnceLock::new();
    C.get_or_init(|| Mutex::new(CString::default()))
}

/// Static buffer for the most recent `elephc_pdo_connection_status` result.
fn connection_status_cell() -> &'static Mutex<CString> {
    static C: OnceLock<Mutex<CString>> = OnceLock::new();
    C.get_or_init(|| Mutex::new(CString::default()))
}

/// Static buffer for the most recent `elephc_pdo_last_insert_id_text` result.
fn last_insert_id_text_cell() -> &'static Mutex<CString> {
    static C: OnceLock<Mutex<CString>> = OnceLock::new();
    C.get_or_init(|| Mutex::new(CString::default()))
}

/// Static buffer for the most recent PostgreSQL text result returned to PHP
/// (`elephc_pdo_lob_create` / `elephc_pdo_copy_out`). Shared because each result is
/// copied into an owned PHP string before the next call writes the cell.
fn pg_text_result_cell() -> &'static Mutex<CString> {
    static C: OnceLock<Mutex<CString>> = OnceLock::new();
    C.get_or_init(|| Mutex::new(CString::default()))
}

/// Static byte buffer for the most recent whole or bounded BLOB / large-object read
/// (`elephc_pdo_blob_read_at`, `elephc_pdo_lob_read_at`, and their legacy whole-value
/// variants), bulk-copied out through
/// `elephc_pdo_blob_data_ptr` (or, on the fallback path, drained byte-by-byte through
/// `elephc_pdo_blob_byte`). A `Vec<u8>` rather than a `CString` because BLOBs are
/// binary and may contain embedded NUL bytes; shared because the prelude copies each
/// result into a PHP string (wrapped in a `php://memory` stream) before the next read
/// overwrites the cell.
fn blob_cell() -> &'static Mutex<Vec<u8>> {
    static C: OnceLock<Mutex<Vec<u8>>> = OnceLock::new();
    C.get_or_init(|| Mutex::new(Vec::new()))
}

/// Stores `s` (NUL bytes stripped) into the per-result static `cell` and returns
/// a pointer into it. Valid until the next call writing the same cell; elephc
/// copies it into an owned PHP string on return.
fn store_cstr(cell: &'static Mutex<CString>, s: &str) -> *const c_char {
    let bytes: Vec<u8> = s.bytes().filter(|&b| b != 0).collect();
    let cstr = CString::new(bytes).unwrap_or_default();
    let mut guard = lock_recover(cell);
    *guard = cstr;
    guard.as_ptr()
}

/// Stores raw bytes into the per-result static data buffer and returns a pointer
/// to the first byte, or null for an empty buffer. Valid until the next column
/// data pointer call; elephc copies it immediately through `ptr_read_string`.
fn store_bytes(bytes: Vec<u8>) -> *const c_char {
    let mut guard = lock_recover(coldata_cell());
    *guard = bytes;
    if guard.is_empty() {
        std::ptr::null()
    } else {
        guard.as_ptr() as *const c_char
    }
}

/// Reads a null-terminated C string argument as a `&str` (the shape elephc's
/// `extern …` string parameters marshal to). Returns `None` for a null pointer
/// or invalid UTF-8.
///
/// # Safety
/// `p`, when non-null, must point to a NUL-terminated string valid for the call.
unsafe fn cstr_arg<'a>(p: *const c_char) -> Option<&'a str> {
    if p.is_null() {
        return None;
    }
    CStr::from_ptr(p).to_str().ok()
}

/// Reads a raw byte-buffer argument (the shape elephc's `extern …` pointer +
/// length parameters marshal to) into an owned `Vec<u8>`. Returns an empty vector
/// for a null pointer or a non-positive length; unlike `cstr_arg` this preserves
/// embedded NUL bytes and does not require valid UTF-8.
///
/// # Safety
/// `p`, when non-null, must point to at least `len` readable bytes valid for the
/// call.
unsafe fn bytes_arg(p: *const c_char, len: i64) -> Vec<u8> {
    if p.is_null() || len <= 0 {
        return Vec::new();
    }
    std::slice::from_raw_parts(p as *const u8, len as usize).to_vec()
}

/// Opens the driver connection for a validated DSN string. `sqlite_open_flags`
/// (P1-10/P2-9) is only consulted for a `sqlite:` DSN; `my_init_command` (P1-9),
/// `my_ssl_config` (the packed `Pdo\Mysql::ATTR_SSL_*` options) and `my_found_rows`
/// (`Pdo\Mysql::ATTR_FOUND_ROWS`, F-MY-06) only for a `mysql:` DSN. PostgreSQL reads
/// its own `sslmode`/`sslrootcert` straight from the DSN, so it takes no extra
/// parameter here; the other driver's parameters are ignored.
fn open_conn_for_dsn(
    dsn: &str,
    sqlite_open_flags: i64,
    my_init_command: &str,
    my_ssl_config: &str,
    my_found_rows: bool,
    my_driver_config: &str,
) -> Result<Conn, String> {
    match driver::DriverKind::from_dsn(dsn) {
        #[cfg(feature = "dblib")]
        Some(driver::DriverKind::Dblib) => dblib::DblibConn::open(dsn).map(Conn::Dblib),
        #[cfg(feature = "firebird")]
        Some(driver::DriverKind::Firebird) => firebird::FirebirdConn::open(dsn).map(Conn::Firebird),
        #[cfg(feature = "odbc")]
        Some(driver::DriverKind::Odbc) => odbc::OdbcConn::open_odbc(dsn).map(Conn::Odbc),
        #[cfg(feature = "informix")]
        Some(driver::DriverKind::Informix) => {
            odbc::OdbcConn::open_informix(dsn).map(Conn::Odbc)
        }
        #[cfg(feature = "oci")]
        Some(driver::DriverKind::Oci) => oci::OciConn::open(dsn).map(Conn::Oci),
        Some(driver::DriverKind::Sqlite) => {
            let path = dsn.strip_prefix(driver::DriverKind::Sqlite.dsn_prefix()).unwrap_or_default();
            sqlite::SqliteConn::open(path, sqlite_open_flags).map(Conn::Sqlite)
        }
        Some(driver::DriverKind::Pgsql) => pg::PgConn::open(dsn).map(Conn::Postgres),
        Some(driver::DriverKind::Mysql) => my::MyConn::open(
            dsn,
            my_init_command,
            my_ssl_config,
            my_found_rows,
            my_driver_config,
        )
        .map(Conn::Mysql),
        None => Err("could not find driver".to_string()),
    }
}

/// Registers a newly opened connection and returns the public handle ID.
fn register_conn(conn: Conn) -> i64 {
    let id = next_id();
    lock_recover(conns()).insert(id, conn);
    id
}

/// Opens a non-persistent connection and stores any failure message for the PDO
/// constructor's `elephc_pdo_last_open_error()` call.
fn open_nonpersistent_dsn(
    dsn: &str,
    sqlite_open_flags: i64,
    my_init_command: &str,
    my_ssl_config: &str,
    my_found_rows: bool,
    my_driver_config: &str,
) -> i64 {
    match open_conn_for_dsn(
        dsn,
        sqlite_open_flags,
        my_init_command,
        my_ssl_config,
        my_found_rows,
        my_driver_config,
    ) {
        Ok(conn) => register_conn(conn),
        Err(msg) => {
            store_open_failure(dsn, &msg);
            -1
        }
    }
}

/// Checks a cached persistent handle using the same driver split as php-src:
/// SQLite needs no probe, MySQL sends COM_PING, and PostgreSQL consults the live
/// client connection state maintained by its connection driver.
fn persistent_connection_is_live(conn_id: i64) -> bool {
    let mut guard = lock_recover(conns());
    match guard.get_mut(&conn_id) {
        #[cfg(feature = "dblib")]
        Some(Conn::Dblib(connection)) => connection.is_alive(),
        #[cfg(feature = "firebird")]
        Some(Conn::Firebird(connection)) => connection.is_alive(),
        #[cfg(any(feature = "odbc", feature = "informix"))]
        Some(Conn::Odbc(connection)) => connection.is_alive(),
        #[cfg(feature = "oci")]
        Some(Conn::Oci(connection)) => connection.is_alive(),
        Some(Conn::Sqlite(_)) => true,
        Some(Conn::Mysql(connection)) => connection.is_alive(),
        Some(Conn::Postgres(connection)) => !connection.is_closed(),
        None => false,
    }
}

/// Evicts a dead persistent connection and every statement that still points to
/// it before a replacement handle is registered.
fn evict_persistent_connection(conn_id: i64) {
    lock_recover(stmts()).retain(|_, statement| match statement {
        #[cfg(feature = "dblib")]
        Stmt::Dblib(statement) => statement.conn_id != conn_id,
        #[cfg(feature = "firebird")]
        Stmt::Firebird(statement) => statement.conn_id != conn_id,
        #[cfg(any(feature = "odbc", feature = "informix"))]
        Stmt::Odbc(statement) => statement.conn_id != conn_id,
        #[cfg(feature = "oci")]
        Stmt::Oci(statement) => statement.conn_id != conn_id,
        Stmt::Sqlite(_) => true,
        Stmt::Postgres(statement) => statement.conn_id != conn_id,
        Stmt::Mysql(statement) => statement.conn_id != conn_id,
    });
    lock_recover(conns()).remove(&conn_id);
    lock_recover(persistent_ids()).remove(&conn_id);
    lock_recover(persistent_owner_counts()).remove(&conn_id);
}

/// Opens or reuses a process-local persistent connection for the `(dsn,
/// persistent_key)` pool key (F-CORE-16; `persistent_key` is `""` for the plain
/// boolean-persistent case — see [`persistent_conns`] for why the key string is part
/// of the key at all).
///
/// `sqlite_open_flags`/`my_init_command`/`my_ssl_config`/`my_found_rows` are only
/// applied on a fresh open: they are not part of the pool key, so a later open
/// reusing an already-pooled connection does not re-apply a different
/// flags/init-command/capability request (matching how no other constructor option
/// retroactively affects a reused persistent handle either — and mirroring php-src,
/// whose hashkey is likewise built from the DSN and the persistent key alone).
fn open_persistent_dsn(
    dsn: &str,
    persistent_key: &str,
    sqlite_open_flags: i64,
    my_init_command: &str,
    my_ssl_config: &str,
    my_found_rows: bool,
    my_driver_config: &str,
) -> i64 {
    let _checkout = lock_recover(persistent_checkout_lock());
    let pool_key = (dsn.to_string(), persistent_key.to_string());
    if let Some(id) = lock_recover(persistent_conns()).get(&pool_key).copied() {
        if persistent_connection_is_live(id) {
            let mut owners = lock_recover(persistent_owner_counts());
            *owners.entry(id).or_insert(0) += 1;
            return id;
        }
        evict_persistent_connection(id);
        lock_recover(persistent_conns()).remove(&pool_key);
    }
    match open_conn_for_dsn(
        dsn,
        sqlite_open_flags,
        my_init_command,
        my_ssl_config,
        my_found_rows,
        my_driver_config,
    ) {
        Ok(conn) => {
            let id = register_conn(conn);
            lock_recover(persistent_conns()).insert(pool_key, id);
            lock_recover(persistent_ids()).insert(id);
            lock_recover(persistent_owner_counts()).insert(id, 1);
            id
        }
        Err(msg) => {
            store_open_failure(dsn, &msg);
            -1
        }
    }
}

/// Returns the bridge ABI version. Bumped when the C ABI shape changes. v7 adds
/// connection/statement SQLSTATE + statement error accessors, boolean/blob binds,
/// a busy-timeout setter, server version reporting, and a text-valued last-insert
/// id. v8 adds the PostgreSQL backend-pid and MySQL warning-count accessors that
/// back `Pdo\Pgsql::getPid()` / `Pdo\Mysql::getWarningCount()`. v9 adds the
/// PostgreSQL large-object create/unlink and COPY in/out accessors backing
/// `Pdo\Pgsql::lobCreate()` / `lobUnlink()` / `copyFrom*()` / `copyTo*()`. v10 adds
/// the SQLite column-decltype and load-extension accessors backing
/// `PDOStatement::getColumnMeta()`'s native type and `Pdo\Sqlite::loadExtension()`.
/// v11 adds the PostgreSQL LISTEN/NOTIFY poll backing `Pdo\Pgsql::getNotify()`.
/// v12 adds the whole-BLOB / whole-large-object read accessors backing
/// `Pdo\Sqlite::openBlob()` / `Pdo\Pgsql::lobOpen()`. v13 adds the SQLite
/// custom-collation registration
/// (`elephc_pdo_create_collation`) backing `Pdo\Sqlite::createCollation()`, whose
/// comparator descriptor and codegen adapter cross as two plain `ptr` arguments.
/// v14 adds the SQLite scalar user-function registration
/// (`elephc_pdo_create_function` + `elephc_pdo_udf_stash_bytes`) backing
/// `Pdo\Sqlite::createFunction()`, sharing the same descriptor/adapter `ptr` shape.
/// v15 adds the SQLite aggregate registration (`elephc_pdo_create_aggregate`) backing
/// `Pdo\Sqlite::createAggregate()`, crossing a step + finalize (descriptor, adapter)
/// pair with the per-group accumulator held in SQLite's aggregate context.
/// v16 adds the PostgreSQL NOTICE drain (`elephc_pdo_get_notice`) backing
/// `Pdo\Pgsql::setNoticeCallback()`, buffered via the connection's `notice_callback`.
/// v17 adds a `sqlite_open_flags` parameter to `elephc_pdo_open_persistent` backing
/// `Pdo\Sqlite::ATTR_OPEN_FLAGS` (P1-10; `0` = no override) and a
/// `sqlite3_stmt_readonly` accessor (`elephc_pdo_stmt_readonly`) backing
/// `PDOStatement::getAttribute(Pdo\Sqlite::ATTR_READONLY_STATEMENT)` (P2-16).
/// v18 adds a `my_init_command` parameter to `elephc_pdo_open_persistent` (P1-9;
/// empty string = none) — one SQL statement run by the MySQL/MariaDB server right
/// after authentication on every (re)connect, backing the minimal wiring for
/// `Pdo\Mysql::ATTR_INIT_COMMAND`; ignored for `sqlite:`/`pgsql:` DSNs.
/// v19 adds a `my_ssl_config` parameter to `elephc_pdo_open_persistent` (empty =
/// no TLS) — the prelude's packed `Pdo\Mysql::ATTR_SSL_*` options applied to the
/// MySQL/MariaDB connection's ring-backed rustls backend (enabled by default) —
/// and enables PostgreSQL TLS via the DSN's own `sslmode`/`sslrootcert`
/// keys through the default `tls` feature's rustls (ring) connector. No new extern
/// is added for pg (its TLS parameters ride the DSN).
/// v20 adds an explicit `len` parameter to `elephc_pdo_bind_text` (the value's
/// true byte length, replacing SQLite's strlen-based `-1` sentinel and pg/mysql's
/// NUL-terminated `cstr_arg` decode) so a bound string with an embedded NUL byte
/// binds in full instead of silently truncating at the first NUL (P0-A); it also
/// routes `PDO::PARAM_LOB` binds through the pre-existing `elephc_pdo_bind_blob`
/// from the prelude, which was implemented but never called.
/// v21 adds `elephc_pdo_no_backslash_escapes`, a live read of whether a `mysql:`
/// connection's session has `NO_BACKSLASH_ESCAPES` active in its `sql_mode`,
/// backing `PDO::quote()`'s MySQL branch (P1-f): under that mode backslash is a
/// literal character in a string literal, so the usual backslash-escaping is
/// unsafe (an escaped quote does not actually escape) and must fall back to
/// `''`-doubling only, matching mysqlnd's own behavior.
/// v22 adds `elephc_pdo_in_transaction`, a live transaction-state read backing
/// `PDO::inTransaction()` / `beginTransaction()`'s already-active guard (P1-g).
/// SQLite reads native autocommit; PostgreSQL/MySQL state is maintained from every
/// successful bridge-owned command because their client crates hide the protocol flag.
/// v23 adds `elephc_pdo_column_native_type` and `elephc_pdo_column_type_oid`,
/// which thread a `pgsql:` result column's `postgres::types::Type` (the server's
/// `pg_type.typname` and `PQftype` OID, resolved at prepare time) through to
/// `PDOStatement::getColumnMeta()` (P2-k). The prelude uses them to report the
/// real PostgreSQL `native_type` (`int4`/`bool`/`bytea`/…), the correct
/// `pdo_type` (BOOL→PARAM_BOOL, int-family→PARAM_INT, BYTEA→PARAM_LOB, else
/// PARAM_STR), and the `pgsql:oid` key, instead of the generic SQLite
/// storage-class metadata it emitted for every driver before. Both return a
/// neutral empty string / `0` for a non-PostgreSQL statement, so SQLite and
/// MySQL keep their existing storage-class metadata path unchanged.
/// v24 adds `elephc_pdo_blob_data_ptr`, which hands back a pointer to the whole
/// shared blob buffer so the prelude can bulk-copy a BLOB / large object with one
/// `ptr_read_string` instead of draining it a byte at a time through
/// `elephc_pdo_blob_byte` (kept as the fallback path), and
/// `elephc_pdo_set_extended_result_codes`, which calls
/// `sqlite3_extended_result_codes` for a `sqlite:` connection to back
/// `Pdo\Sqlite::ATTR_EXTENDED_RESULT_CODES` (F-SQLT-02). It REMOVES
/// `elephc_pdo_column_text` (F-QUAL-03): it was declared by the prelude but never
/// called, and it silently stripped embedded NUL bytes on the way out — every live
/// column read goes through the NUL-preserving
/// `elephc_pdo_column_data_len`/`elephc_pdo_column_data_ptr` pair instead.
/// v25 adds two trailing parameters to `elephc_pdo_open_persistent`. `my_found_rows`
/// (`0`/`1`) ORs `CLIENT_FOUND_ROWS` into a `mysql:` connection's negotiated
/// capabilities, backing `Pdo\Mysql::ATTR_FOUND_ROWS` (F-MY-06): it switches an
/// UPDATE's `rowCount()` from "rows actually CHANGED" to "rows MATCHED by the WHERE
/// clause", a capability that can only be selected in the connect handshake; it is
/// ignored for `sqlite:`/`pgsql:` DSNs. `persistent_key` is the caller's
/// `PDO::ATTR_PERSISTENT` string when that option was given as a non-numeric,
/// non-empty string (else `""`), and now forms the persistent pool key TOGETHER with
/// the DSN (F-CORE-16): php-src builds its persistent hashkey from both
/// (`pdo_dbh.c:389-404`), so two persistent connections to the same DSN under
/// different key strings are DISTINCT pooled entries rather than one wrongly shared
/// connection. It is ignored when `persistent` is `0`.
/// v26 adds the three remaining PostgreSQL column-metadata accessors —
/// `elephc_pdo_column_table_oid` (`PQftable`), `elephc_pdo_column_len` (`PQfsize`)
/// and `elephc_pdo_column_precision` (`PQfmod`) — which back `getColumnMeta`'s
/// `pgsql:table_oid`, `len` and `precision` keys (F-PG-01, F-PG-02). php-src emits
/// `pgsql:table_oid` UNCONDITIONALLY, `0` (`InvalidOid`, i.e. "not a plain table
/// column") included, so the prelude emits the key even for a `0`; `len` is the
/// TYPE's byte width (`int4` → 4, `timestamp` → 8) and `-1` for any varlena
/// (`text`, `varchar`, `numeric`, `bytea`, `json`, arrays), whose declared `n`
/// surfaces instead through `precision` as the RAW, undecoded `atttypmod`
/// (`VARCHAR(20)` → 24) — exactly as php-src stores them. All three return their
/// PostgreSQL-neutral value (`0` / `-1` / `-1`) for a non-`pgsql:` statement, so
/// SQLite and MySQL are untouched. v26 also extends `elephc_pdo_column_native_type`
/// to `mysql:` statements, which previously fell through to the empty string and
/// therefore reported the generic storage-class metadata: a MySQL column now
/// reports php-src's real `type_to_name_native` name
/// (`ext/pdo_mysql/mysql_statement.c:716-770`) — `LONG`, `VAR_STRING`, `BIT`,
/// `NEWDECIMAL`, `BLOB`, … — the stringified `MYSQL_TYPE_` suffix rather than the
/// friendlier SQL spelling (F-MY-08); a wire type php-src's own switch has no case
/// for still yields the empty string, matching its `default: return NULL`, which
/// makes php-src omit the key entirely.
/// v27 adds the `emulated` argument to `elephc_pdo_prepare`. MySQL uses the text
/// protocol when it is non-zero; PostgreSQL uses the simple-query protocol; SQLite
/// ignores it. This makes `PDO::ATTR_EMULATE_PREPARES`, MySQL direct-query mode and
/// `Pdo\Pgsql::ATTR_DISABLE_PREPARES` select a real protocol path rather than an
/// echo-only attribute. v28 adds `elephc_pdo_stmt_sent_sql`, exposing the most
/// recently rendered emulated SQL so `PDOStatement::debugDumpParams()` can print
/// php-src's `Sent SQL:` line without duplicating either driver's quoting logic.
/// v29 adds PHP 8.5 SQLite transaction, busy-statement, and explain-statement accessors.
/// v30 adds PHP 8.5 SQLite authorizer registration and nullable reset. v31 adds
/// live MySQL `PDO::ATTR_AUTOCOMMIT` mutation and state reads. v32 adds national
/// string binds for MySQL `PDO::ATTR_DEFAULT_STR_PARAM` / `PARAM_STR_NATL`. v33
/// adds deferred SQLite authorizer callback error classification. v34 retains
/// every MySQL protocol result set and exposes `elephc_pdo_next_rowset`. v35 adds
/// `elephc_pdo_clear_callbacks`, which unregisters every SQLite native callback
/// before persistent PDO objects release their compiled callable descriptor roots.
/// v36 adds live client-version, server-information, and connection-status string
/// accessors for the generic PDO attributes. v37 adds PostgreSQL scroll-cursor
/// orientation stepping. v38 adds live MySQL table-name prefix configuration.
/// v39 adds PostgreSQL result-memory accounting. v40 adds binary-safe SQLite BLOB
/// and PostgreSQL large-object writeback for the seekable PDO stream wrappers.
/// v41 adds packed pdo_mysql connection options and buffered-query accessors.
/// v42 adds PostgreSQL connection/statement prefetch controls.
/// v43 adds source-table names and MySQL column flags. v44 adds version-aware
/// persistent-handle release so PHP 8.6 can reset PostgreSQL session state. v45
/// adds bounded PostgreSQL large-object size/read/write operations. v46 adds the
/// equivalent bounded size/read/write operations for SQLite incremental BLOBs.
/// v47 enables PHP 8.5+'s demand-driven simple-query protocol per statement.
/// v48 exposes the compiled-driver registry and runtime `pdo.dsn.*` INI aliases.
/// v49 adds the optional PDO_DBLIB driver and its live attribute controls. v50
/// adds the optional PDO_FIREBIRD backend and its driver-specific attributes.
/// v51 adds the optional PDO_ODBC backend through the system driver manager.
/// v52 adds PDO_OCI through Oracle Instant Client plus OCI attributes and metadata.
/// v53 adds OCI input/output bind registration and binary-safe output retrieval.
/// v54 exposes PDO_OCI constructor SQLSTATE and native ORA diagnostics.
/// v55 adds PDO_INFORMIX column scale/type/flag metadata and widens the generic
/// native-type/table-name accessors to the shared CLI backend.
#[no_mangle]
pub extern "C" fn elephc_pdo_version() -> i32 {
    // Guarded like every other extern purely for uniformity — "every `#[no_mangle]`
    // body opens with `ffi_guard`" is a grep-checkable invariant, and a constant
    // body simply never reaches the fallback.
    ffi_guard(55, || 55)
}

/// Returns a pointer to the lowercase PDO driver name for a connection
/// (`"sqlite"`, `"pgsql"`, or `"mysql"`), or an empty string for an unknown
/// handle. Backs `PDO::getAttribute(PDO::ATTR_DRIVER_NAME)`. Valid until the next
/// `elephc_pdo_driver_name`. A caught panic degrades to the same empty string.
#[no_mangle]
pub extern "C" fn elephc_pdo_driver_name(conn_id: i64) -> *const c_char {
    ffi_guard(static_cstr(b"\0"), || {
        let name = lock_recover(conns())
            .get(&conn_id)
            .map(Conn::driver_kind)
            .map(driver::DriverKind::name)
            .unwrap_or_default();
        store_cstr(drivername_cell(), name)
    })
}

/// Returns the number of PDO drivers compiled into this bridge.
#[no_mangle]
pub extern "C" fn elephc_pdo_available_driver_count() -> i64 {
    ffi_guard(0, || driver::AVAILABLE.len() as i64)
}

/// Returns the lowercase name of the available driver at `index`, or an empty
/// string when `index` is outside the registry. The pointer remains valid for the
/// process lifetime because registry names are static string literals.
#[no_mangle]
pub extern "C" fn elephc_pdo_available_driver_name(index: i64) -> *const c_char {
    ffi_guard(static_cstr(b"\0"), || {
        let Some(kind) = usize::try_from(index)
            .ok()
            .and_then(|index| driver::AVAILABLE.get(index))
        else {
            return static_cstr(b"\0");
        };
        match kind {
            #[cfg(feature = "dblib")]
            driver::DriverKind::Dblib => static_cstr(b"dblib\0"),
            #[cfg(feature = "firebird")]
            driver::DriverKind::Firebird => static_cstr(b"firebird\0"),
            #[cfg(feature = "odbc")]
            driver::DriverKind::Odbc => static_cstr(b"odbc\0"),
            #[cfg(feature = "informix")]
            driver::DriverKind::Informix => static_cstr(b"informix\0"),
            #[cfg(feature = "oci")]
            driver::DriverKind::Oci => static_cstr(b"oci\0"),
            driver::DriverKind::Mysql => static_cstr(b"mysql\0"),
            driver::DriverKind::Pgsql => static_cstr(b"pgsql\0"),
            driver::DriverKind::Sqlite => static_cstr(b"sqlite\0"),
        }
    })
}

/// Returns `1` when runtime PHP configuration defines `pdo.dsn.<name>`, and `0`
/// otherwise. Alias names are case-sensitive like php-src's configuration table.
///
/// # Safety
/// `name` must point to a NUL-terminated string valid for the duration of the call.
#[no_mangle]
pub unsafe extern "C" fn elephc_pdo_ini_dsn_defined(name: *const c_char) -> i64 {
    ffi_guard(0, || {
        cstr_arg(name)
            .and_then(ini::lookup)
            .map_or(0, |_| 1)
    })
}

/// Returns the configured value of `pdo.dsn.<name>`, or an empty string for an
/// absent/invalid name. Callers use `elephc_pdo_ini_dsn_defined()` to distinguish
/// an absent alias from a deliberately configured empty value.
///
/// # Safety
/// `name` must point to a NUL-terminated string valid for the duration of the call.
#[no_mangle]
pub unsafe extern "C" fn elephc_pdo_ini_dsn_value(name: *const c_char) -> *const c_char {
    ffi_guard(static_cstr(b"\0"), || {
        let value = cstr_arg(name).and_then(ini::lookup).unwrap_or_default();
        store_cstr(dsn_alias_cell(), value)
    })
}

/// Opens a non-persistent database for a PDO DSN, dispatching on the driver
/// prefix. Returns an `i64` connection handle, or `-1` on failure with the
/// message stashed for `elephc_pdo_last_open_error`. A caught panic degrades to the
/// same `-1` (with whatever message was last stashed) rather than aborting.
///
/// # Safety
/// `dsn` must point to a NUL-terminated string valid for the duration of the call.
#[no_mangle]
pub unsafe extern "C" fn elephc_pdo_open(dsn: *const c_char) -> i64 {
    ffi_guard(-1, || {
        let Some(dsn) = cstr_arg(dsn) else {
            store_open_failure("", "invalid DSN");
            return -1;
        };
        open_nonpersistent_dsn(dsn, 0, "", "", false, "")
    })
}

/// Opens a database for a PDO DSN, reusing a process-local pooled connection when
/// `persistent` is non-zero. Persistent handles stay registered until process
/// exit; release only decrements their live-owner count. `sqlite_open_flags` (v17) is the
/// raw `sqlite3_open_v2` flags to open a `sqlite:` DSN with — `0` means "use the
/// default `READWRITE|CREATE`" — and is ignored for PostgreSQL/MySQL DSNs; it backs
/// `Pdo\Sqlite::ATTR_OPEN_FLAGS` (P1-10). `my_init_command` (v18) is a SQL
/// statement run right after authentication on a `mysql:` connection (empty = do
/// nothing), ignored for SQLite/PostgreSQL DSNs; it backs the minimal wiring for
/// `Pdo\Mysql::ATTR_INIT_COMMAND` (P1-9). `my_ssl_config` (v19) is the prelude's
/// packed `Pdo\Mysql::ATTR_SSL_*` options (`ca=…;cert=…;key=…;verify=0|1`, empty =
/// no TLS) applied to the `mysql:` connection's rustls backend; it is ignored for
/// SQLite/PostgreSQL DSNs (PostgreSQL carries its own `sslmode`/`sslrootcert` in
/// the DSN) and requires the default `mysql-tls` feature to take effect.
/// `my_found_rows` (v25) is `1` to OR `CLIENT_FOUND_ROWS` into a `mysql:`
/// connection's negotiated capabilities and `0` not to; it backs
/// `Pdo\Mysql::ATTR_FOUND_ROWS` (F-MY-06), which makes an UPDATE's `rowCount()`
/// report the rows its WHERE clause MATCHED instead of the rows it actually CHANGED,
/// and is ignored for SQLite/PostgreSQL DSNs. `persistent_key` (v25) is the caller's
/// `PDO::ATTR_PERSISTENT` string when that option was a non-numeric, non-empty string
/// (`""` otherwise, and for the plain boolean-persistent case); together with the DSN
/// it forms the persistent pool key (F-CORE-16), and it is ignored when `persistent`
/// is `0`. A caught panic degrades to the same `-1` failure sentinel as a failed open.
///
/// # Safety
/// `dsn` and, when non-null, `my_init_command`/`my_ssl_config`/`persistent_key` must
/// each point to a NUL-terminated string valid for the duration of the call.
#[no_mangle]
pub unsafe extern "C" fn elephc_pdo_open_persistent(
    dsn: *const c_char,
    persistent: i64,
    sqlite_open_flags: i64,
    my_init_command: *const c_char,
    my_ssl_config: *const c_char,
    my_found_rows: i64,
    persistent_key: *const c_char,
    my_driver_config: *const c_char,
) -> i64 {
    ffi_guard(-1, || {
        let Some(dsn) = cstr_arg(dsn) else {
            store_open_failure("", "invalid DSN");
            return -1;
        };
        let init_command = cstr_arg(my_init_command).unwrap_or("");
        let ssl_config = cstr_arg(my_ssl_config).unwrap_or("");
        let found_rows = my_found_rows != 0;
        let driver_config = cstr_arg(my_driver_config).unwrap_or("");
        if persistent == 0 {
            open_nonpersistent_dsn(
                dsn,
                sqlite_open_flags,
                init_command,
                ssl_config,
                found_rows,
                driver_config,
            )
        } else {
            // A null / non-UTF-8 key degrades to `""`, i.e. the plain
            // boolean-persistent pool for this DSN — the pre-v25 behavior.
            let key = cstr_arg(persistent_key).unwrap_or("");
            open_persistent_dsn(
                dsn,
                key,
                sqlite_open_flags,
                init_command,
                ssl_config,
                found_rows,
                driver_config,
            )
        }
    })
}

/// Returns a pointer to the message captured by the most recent failed
/// `elephc_pdo_open`. Valid until the next failed open. A caught panic degrades to
/// an empty message.
#[no_mangle]
pub extern "C" fn elephc_pdo_last_open_error() -> *const c_char {
    ffi_guard(static_cstr(b"\0"), || {
        lock_recover(open_error_cell()).as_ptr()
    })
}

/// Returns the SQLSTATE captured for the most recent failed open, or an empty
/// string when that driver did not expose a constructor diagnostic.
#[no_mangle]
pub extern "C" fn elephc_pdo_last_open_sqlstate() -> *const c_char {
    ffi_guard(static_cstr(b"\0"), || {
        lock_recover(open_sqlstate_cell()).as_ptr()
    })
}

/// Returns the native driver code captured for the most recent failed open.
#[no_mangle]
pub extern "C" fn elephc_pdo_last_open_native_code() -> i64 {
    ffi_guard(0, || open_native_code_cell().load(Ordering::Relaxed))
}

/// Releases one PDO owner of `conn_id`. Non-persistent connections are closed;
/// pooled handles remain cached. When the final owner selects PHP 8.6 reset
/// semantics, PostgreSQL performs its disconnect-equivalent session cleanup.
fn release_connection(conn_id: i64, reset_pgsql_session: bool) {
    if lock_recover(persistent_ids()).contains(&conn_id) {
        let became_idle = {
            let mut owners = lock_recover(persistent_owner_counts());
            let count = owners.entry(conn_id).or_insert(0);
            if *count == 0 {
                false
            } else {
                *count -= 1;
                *count == 0
            }
        };
        if became_idle && reset_pgsql_session {
            lock_recover(stmts()).retain(|_, statement| match statement {
                Stmt::Postgres(statement) => statement.conn_id != conn_id,
                _ => true,
            });
            if let Some(Conn::Postgres(connection)) = lock_recover(conns()).get_mut(&conn_id) {
                connection.discard_all();
            }
        }
        return;
    }
        // The SQLite db pointer of the connection being closed, so only *its*
        // statements are finalized (statements from other open SQLite connections
        // must be left alone). `None` when the connection is PostgreSQL or unknown.
        let sqlite_db = match lock_recover(conns()).get(&conn_id) {
            Some(Conn::Sqlite(c)) => Some(c.db),
            _ => None,
        };
        // Finalize and drop the statements belonging to this connection so
        // sqlite3_close does not fail with SQLITE_BUSY; PostgreSQL/MySQL statements
        // live server-side and are dropped with the client.
        let owned: Vec<i64> = lock_recover(stmts())
            .iter()
            .filter_map(|(k, s)| match s {
                Stmt::Sqlite(st) if sqlite_db == Some(st.db) => Some(*k),
                Stmt::Postgres(p) if p.conn_id == conn_id => Some(*k),
                Stmt::Mysql(m) if m.conn_id == conn_id => Some(*k),
                #[cfg(feature = "firebird")]
                Stmt::Firebird(f) if f.conn_id == conn_id => Some(*k),
                _ => None,
            })
            .collect();
        {
            let mut guard = lock_recover(stmts());
            for k in owned {
                if let Some(Stmt::Sqlite(s)) = guard.get(&k) {
                    s.finalize();
                }
                guard.remove(&k);
            }
        }
        if let Some(Conn::Sqlite(c)) = lock_recover(conns()).get(&conn_id) {
            c.close();
        }
        lock_recover(conns()).remove(&conn_id);
}

/// Closes a connection using PHP 8.0-8.5 persistent-session semantics. Unknown
/// handles and caught panics are ignored because destructors have no error channel.
#[no_mangle]
pub extern "C" fn elephc_pdo_close(conn_id: i64) {
    ffi_guard((), || release_connection(conn_id, false))
}

/// Releases a connection with a version-selected PostgreSQL persistent reset.
/// `reset_pgsql_session != 0` is emitted only for the PHP 8.6 compatibility target.
#[no_mangle]
pub extern "C" fn elephc_pdo_release(conn_id: i64, reset_pgsql_session: i64) {
    ffi_guard((), || {
        release_connection(conn_id, reset_pgsql_session != 0)
    })
}

/// Runs one or more SQL statements with no result rows (`PDO::exec`). Returns the
/// number of rows changed, or `-1` on error — the same sentinel a caught panic
/// degrades to.
///
/// # Safety
/// `sql` must point to a NUL-terminated string valid for the duration of the call.
#[no_mangle]
pub unsafe extern "C" fn elephc_pdo_exec(conn_id: i64, sql: *const c_char) -> i64 {
    ffi_guard(-1, || {
        let sqlite_db = match lock_recover(conns()).get(&conn_id) {
            Some(Conn::Sqlite(connection)) => Some(connection.db),
            _ => None,
        };
        if let Some(db) = sqlite_db {
            return sqlite::SqliteConn::exec_on(db, sql);
        }
        let mut guard = lock_recover(conns());
        match guard.get_mut(&conn_id) {
            #[cfg(feature = "dblib")]
            Some(Conn::Dblib(c)) => match cstr_arg(sql) {
                Some(sql) => c.execute(sql).map_or(-1, |_| c.changes),
                None => -1,
            },
            #[cfg(feature = "firebird")]
            Some(Conn::Firebird(c)) => match cstr_arg(sql) {
                Some(sql) => c.execute(sql, Vec::new()).map_or(-1, |_| c.changes),
                None => -1,
            },
            #[cfg(any(feature = "odbc", feature = "informix"))]
            Some(Conn::Odbc(c)) => match cstr_arg(sql) {
                Some(sql) => c.exec(sql),
                None => -1,
            },
            #[cfg(feature = "oci")]
            Some(Conn::Oci(c)) => match cstr_arg(sql) {
                Some(sql) => c.exec(sql),
                None => -1,
            },
            Some(Conn::Postgres(c)) => match cstr_arg(sql) {
                Some(s) => c.exec(s),
                None => -1,
            },
            Some(Conn::Mysql(c)) => match cstr_arg(sql) {
                Some(s) => c.exec(s),
                None => -1,
            },
            Some(Conn::Sqlite(_)) | None => -1,
        }
    })
}

/// Returns the id of the most recent INSERT: the SQLite rowid, or for PostgreSQL
/// `currval(name)` when a non-empty sequence name is given else `lastval()`.
/// Unknown handles — and a caught panic — report `0`.
///
/// # Safety
/// `name`, when non-null, must point to a NUL-terminated string valid for the call.
#[no_mangle]
pub unsafe extern "C" fn elephc_pdo_last_insert_id(conn_id: i64, name: *const c_char) -> i64 {
    ffi_guard(0, || {
        let mut guard = lock_recover(conns());
        match guard.get_mut(&conn_id) {
            #[cfg(feature = "dblib")]
            Some(Conn::Dblib(c)) => c
                .execute("SELECT @@IDENTITY")
                .ok()
                .and_then(|sets| sets.into_iter().next())
                .and_then(|set| set.rows.into_iter().next())
                .and_then(|row| row.into_iter().next())
                .and_then(|cell| match cell {
                    dblib::DblibCell::Int(value) => Some(value),
                    dblib::DblibCell::Float(value) => Some(value as i64),
                    dblib::DblibCell::Bytes(value, _) => String::from_utf8(value).ok()?.parse().ok(),
                    dblib::DblibCell::Null => None,
                })
                .unwrap_or(0),
            #[cfg(feature = "firebird")]
            Some(Conn::Firebird(_)) => 0,
            #[cfg(any(feature = "odbc", feature = "informix"))]
            Some(Conn::Odbc(c)) => c.last_insert_id().parse().unwrap_or(0),
            #[cfg(feature = "oci")]
            Some(Conn::Oci(_)) => 0,
            Some(Conn::Sqlite(c)) => c.last_insert_id(),
            Some(Conn::Postgres(c)) => c.last_insert_id(cstr_arg(name)),
            Some(Conn::Mysql(c)) => c.last_insert_id(cstr_arg(name)),
            None => 0,
        }
    })
}

/// Like `elephc_pdo_last_insert_id`, but returns a pointer to the id rendered as
/// text: PostgreSQL sequence values are not always safe to round-trip as `i64`
/// (a caller-chosen sequence can be any integer type), so text avoids a lossy or
/// failing numeric bridge; likewise (P2-2) a MySQL `BIGINT UNSIGNED`
/// AUTO_INCREMENT id can exceed `i64::MAX`. Empty string on an unknown handle or
/// error — the same answer a caught panic degrades to. Valid until the next
/// `elephc_pdo_last_insert_id_text`.
///
/// # Safety
/// `name`, when non-null, must point to a NUL-terminated string valid for the call.
#[no_mangle]
pub unsafe extern "C" fn elephc_pdo_last_insert_id_text(
    conn_id: i64,
    name: *const c_char,
) -> *const c_char {
    ffi_guard(static_cstr(b"\0"), || {
        let text = {
            let mut guard = lock_recover(conns());
            match guard.get_mut(&conn_id) {
                #[cfg(feature = "dblib")]
                Some(Conn::Dblib(c)) => c
                    .execute("SELECT @@IDENTITY")
                    .ok()
                    .and_then(|sets| sets.into_iter().next())
                    .and_then(|set| set.rows.into_iter().next())
                    .and_then(|row| row.into_iter().next())
                    .map(|cell| match cell {
                        dblib::DblibCell::Int(value) => value.to_string(),
                        dblib::DblibCell::Float(value) => value.to_string(),
                        dblib::DblibCell::Bytes(value, _) => String::from_utf8_lossy(&value).into_owned(),
                        dblib::DblibCell::Null => String::new(),
                    })
                    .unwrap_or_default(),
                #[cfg(feature = "firebird")]
                Some(Conn::Firebird(_)) => String::new(),
                #[cfg(any(feature = "odbc", feature = "informix"))]
                Some(Conn::Odbc(c)) => c.last_insert_id(),
                #[cfg(feature = "oci")]
                Some(Conn::Oci(_)) => String::new(),
                Some(Conn::Sqlite(c)) => c.last_insert_id().to_string(),
                Some(Conn::Postgres(c)) => c.last_insert_id_text(cstr_arg(name)),
                Some(Conn::Mysql(c)) => c.last_insert_id_text(cstr_arg(name)),
                None => String::new(),
            }
        };
        store_cstr(last_insert_id_text_cell(), &text)
    })
}

/// Returns the number of rows changed by the most recent statement. Unknown handles
/// — and a caught panic — report `0`.
#[no_mangle]
pub extern "C" fn elephc_pdo_changes(conn_id: i64) -> i64 {
    ffi_guard(0, || {
        let guard = lock_recover(conns());
        match guard.get(&conn_id) {
            #[cfg(feature = "dblib")]
            Some(Conn::Dblib(c)) => c.changes,
            #[cfg(feature = "firebird")]
            Some(Conn::Firebird(c)) => c.changes,
            #[cfg(any(feature = "odbc", feature = "informix"))]
            Some(Conn::Odbc(c)) => c.changes,
            #[cfg(feature = "oci")]
            Some(Conn::Oci(c)) => c.changes,
            Some(Conn::Sqlite(c)) => c.changes(),
            Some(Conn::Postgres(c)) => c.changes,
            Some(Conn::Mysql(c)) => c.changes,
            None => 0,
        }
    })
}

/// Begins a transaction (`PDO::beginTransaction`). Returns `1`/`0`; a caught panic
/// reports the `0` failure sentinel.
#[no_mangle]
pub extern "C" fn elephc_pdo_begin(conn_id: i64) -> i64 {
    ffi_guard(0, || {
        let mut guard = lock_recover(conns());
        match guard.get_mut(&conn_id) {
            #[cfg(feature = "dblib")]
            Some(Conn::Dblib(c)) => c.transaction("BEGIN TRANSACTION", true) as i64,
            #[cfg(feature = "firebird")]
            Some(Conn::Firebird(c)) => c.begin() as i64,
            #[cfg(any(feature = "odbc", feature = "informix"))]
            Some(Conn::Odbc(c)) => c.begin() as i64,
            #[cfg(feature = "oci")]
            Some(Conn::Oci(c)) => c.begin() as i64,
            Some(Conn::Sqlite(c)) => c.begin_transaction(),
            Some(Conn::Postgres(c)) => c.exec_simple("BEGIN"),
            Some(Conn::Mysql(c)) => c.exec_simple("BEGIN"),
            None => 0,
        }
    })
}

/// Commits the active transaction (`PDO::commit`). Returns `1`/`0`; a caught panic
/// reports the `0` failure sentinel.
#[no_mangle]
pub extern "C" fn elephc_pdo_commit(conn_id: i64) -> i64 {
    ffi_guard(0, || {
        let mut guard = lock_recover(conns());
        match guard.get_mut(&conn_id) {
            #[cfg(feature = "dblib")]
            Some(Conn::Dblib(c)) => c.transaction("COMMIT TRANSACTION", false) as i64,
            #[cfg(feature = "firebird")]
            Some(Conn::Firebird(c)) => c.commit() as i64,
            #[cfg(any(feature = "odbc", feature = "informix"))]
            Some(Conn::Odbc(c)) => c.commit() as i64,
            #[cfg(feature = "oci")]
            Some(Conn::Oci(c)) => c.commit() as i64,
            Some(Conn::Sqlite(c)) => c.exec_simple(b"COMMIT"),
            Some(Conn::Postgres(c)) => c.exec_simple("COMMIT"),
            Some(Conn::Mysql(c)) => c.exec_simple("COMMIT"),
            None => 0,
        }
    })
}

/// Rolls back the active transaction (`PDO::rollBack`). Returns `1`/`0`; a caught
/// panic reports the `0` failure sentinel.
#[no_mangle]
pub extern "C" fn elephc_pdo_rollback(conn_id: i64) -> i64 {
    ffi_guard(0, || {
        let mut guard = lock_recover(conns());
        match guard.get_mut(&conn_id) {
            #[cfg(feature = "dblib")]
            Some(Conn::Dblib(c)) => c.transaction("ROLLBACK TRANSACTION", false) as i64,
            #[cfg(feature = "firebird")]
            Some(Conn::Firebird(c)) => c.rollback() as i64,
            #[cfg(any(feature = "odbc", feature = "informix"))]
            Some(Conn::Odbc(c)) => c.rollback() as i64,
            #[cfg(feature = "oci")]
            Some(Conn::Oci(c)) => c.rollback() as i64,
            Some(Conn::Sqlite(c)) => c.exec_simple(b"ROLLBACK"),
            Some(Conn::Postgres(c)) => c.exec_simple("ROLLBACK"),
            Some(Conn::Mysql(c)) => c.exec_simple("ROLLBACK"),
            None => 0,
        }
    })
}

/// Returns the connection's LIVE transaction state (P1-g), so a transaction
/// started via a raw `exec("BEGIN")` — bypassing `PDO::beginTransaction()` — is
/// still visible to `PDO::inTransaction()` and to `beginTransaction()`'s
/// already-active guard. `1` = definitely in a transaction, `0` = definitely
/// not; `-1` = unknown because the handle is unrecognized — the prelude falls back to its own `$inTxn` flag in
/// that case. SQLite reads `sqlite3_get_autocommit` live. MySQL/MariaDB and
/// PostgreSQL expose bridge-maintained state updated after every successful command,
/// including raw `BEGIN`/`COMMIT`/`ROLLBACK` sent through `PDO::exec`.
/// A caught panic degrades to that same `-1` ("unknown"), which the prelude
/// already knows how to fall back from.
#[no_mangle]
pub extern "C" fn elephc_pdo_in_transaction(conn_id: i64) -> i64 {
    ffi_guard(-1, || {
        let guard = lock_recover(conns());
        match guard.get(&conn_id) {
            #[cfg(feature = "dblib")]
            Some(Conn::Dblib(c)) => c.in_transaction as i64,
            #[cfg(feature = "firebird")]
            Some(Conn::Firebird(c)) => c.in_transaction as i64,
            #[cfg(any(feature = "odbc", feature = "informix"))]
            Some(Conn::Odbc(c)) => c.in_transaction as i64,
            #[cfg(feature = "oci")]
            Some(Conn::Oci(c)) => c.in_transaction as i64,
            Some(Conn::Sqlite(c)) => c.in_transaction(),
            Some(Conn::Postgres(c)) => c.in_transaction as i64,
            Some(Conn::Mysql(c)) => c.in_transaction as i64,
            None => -1,
        }
    })
}

/// Sets MySQL session autocommit and returns `1` on success. SQLite and
/// PostgreSQL do not expose this attribute through their php-src driver hooks,
/// so non-MySQL or unknown handles return `0`.
#[no_mangle]
pub extern "C" fn elephc_pdo_set_autocommit(conn_id: i64, enabled: i64) -> i64 {
    ffi_guard(0, || match lock_recover(conns()).get_mut(&conn_id) {
        Some(Conn::Mysql(c)) => c.set_autocommit(enabled != 0),
        #[cfg(any(feature = "odbc", feature = "informix"))]
        Some(Conn::Odbc(c)) => c.set_attribute(0, enabled) as i64,
        #[cfg(feature = "oci")]
        Some(Conn::Oci(c)) => c.set_attribute_int(0, enabled) as i64,
        _ => 0,
    })
}

/// Returns MySQL's current session autocommit state, or `-1` for another
/// driver/unknown handle so the prelude can route unsupported attributes normally.
#[no_mangle]
pub extern "C" fn elephc_pdo_autocommit(conn_id: i64) -> i64 {
    ffi_guard(-1, || match lock_recover(conns()).get_mut(&conn_id) {
        Some(Conn::Mysql(c)) => c.autocommit as i64,
        #[cfg(any(feature = "odbc", feature = "informix"))]
        Some(Conn::Odbc(c)) => c.attribute(0).unwrap_or(-1),
        #[cfg(feature = "oci")]
        Some(Conn::Oci(c)) => c.attribute_int(0).unwrap_or(-1),
        _ => -1,
    })
}

/// Enables or disables MySQL `PDO::ATTR_FETCH_TABLE_NAMES`; returns `1` for a
/// MySQL handle and `0` for another driver or an unknown handle.
#[no_mangle]
pub extern "C" fn elephc_pdo_set_fetch_table_names(conn_id: i64, enabled: i64) -> i64 {
    ffi_guard(0, || match lock_recover(conns()).get_mut(&conn_id) {
        Some(Conn::Mysql(connection)) => {
            connection.set_fetch_table_names(enabled != 0);
            1
        }
        _ => 0,
    })
}

/// Returns MySQL's current table-name prefix setting, or `-1` for another driver
/// or an unknown handle.
#[no_mangle]
pub extern "C" fn elephc_pdo_fetch_table_names(conn_id: i64) -> i64 {
    ffi_guard(-1, || match lock_recover(conns()).get(&conn_id) {
        Some(Conn::Mysql(connection)) => connection.fetch_table_names as i64,
        _ => -1,
    })
}

/// Sets MySQL's default buffered-query mode for subsequently prepared
/// statements, returning 1 for a MySQL handle and 0 otherwise.
#[no_mangle]
pub extern "C" fn elephc_pdo_set_buffered_query(conn_id: i64, enabled: i64) -> i64 {
    ffi_guard(0, || match lock_recover(conns()).get_mut(&conn_id) {
        Some(Conn::Mysql(connection)) => connection.set_buffered_query(enabled != 0),
        _ => 0,
    })
}

/// Returns MySQL's current `ATTR_USE_BUFFERED_QUERY` default, or -1 when the
/// connection is unknown or belongs to another driver.
#[no_mangle]
pub extern "C" fn elephc_pdo_buffered_query(conn_id: i64) -> i64 {
    ffi_guard(-1, || match lock_recover(conns()).get(&conn_id) {
        Some(Conn::Mysql(connection)) => connection.buffered_query(),
        _ => -1,
    })
}

/// Sets PostgreSQL's default `PDO::ATTR_PREFETCH` mode for subsequently
/// prepared statements, returning 1 for a PostgreSQL handle and 0 otherwise.
#[no_mangle]
pub extern "C" fn elephc_pdo_set_prefetch(conn_id: i64, enabled: i64) -> i64 {
    ffi_guard(0, || match lock_recover(conns()).get_mut(&conn_id) {
        Some(Conn::Postgres(connection)) => connection.set_prefetch(enabled != 0),
        #[cfg(feature = "oci")]
        Some(Conn::Oci(connection)) => connection.set_attribute_int(1, enabled) as i64,
        _ => 0,
    })
}

/// Overrides one unexecuted PostgreSQL statement's prepare-time prefetch mode.
#[no_mangle]
pub extern "C" fn elephc_pdo_stmt_set_prefetch(stmt_id: i64, enabled: i64) -> i64 {
    ffi_guard(0, || match lock_recover(stmts()).get_mut(&stmt_id) {
        Some(Stmt::Postgres(statement)) => statement.set_prefetch(enabled != 0),
        #[cfg(feature = "oci")]
        Some(Stmt::Oci(statement)) => statement.set_prefetch(enabled),
        _ => 0,
    })
}

/// Enables lazy simple-protocol consumption for a PostgreSQL statement compiled
/// against PHP 8.5 or newer. Other drivers and already-executed statements reject it.
#[no_mangle]
pub extern "C" fn elephc_pdo_stmt_enable_simple_streaming(stmt_id: i64) -> i64 {
    ffi_guard(0, || match lock_recover(stmts()).get_mut(&stmt_id) {
        Some(Stmt::Postgres(statement)) => statement.enable_simple_streaming(),
        _ => 0,
    })
}

/// Returns the driver's result code for the connection's last operation. Unknown
/// handles — and a caught panic — report `-1`.
#[no_mangle]
pub extern "C" fn elephc_pdo_errcode(conn_id: i64) -> i64 {
    ffi_guard(-1, || {
        let guard = lock_recover(conns());
        match guard.get(&conn_id) {
            #[cfg(feature = "dblib")]
            Some(Conn::Dblib(c)) => c.errcode(),
            #[cfg(feature = "firebird")]
            Some(Conn::Firebird(c)) => c.errcode(),
            #[cfg(any(feature = "odbc", feature = "informix"))]
            Some(Conn::Odbc(c)) => c.errcode(),
            #[cfg(feature = "oci")]
            Some(Conn::Oci(c)) => c.errcode(),
            Some(Conn::Sqlite(c)) => c.errcode(),
            Some(Conn::Postgres(c)) => c.errcode,
            Some(Conn::Mysql(c)) => c.errcode,
            None => -1,
        }
    })
}

/// Returns a pointer to the connection's current error message. Valid until the
/// next `elephc_pdo_errmsg`. A caught panic degrades to an empty message.
#[no_mangle]
pub extern "C" fn elephc_pdo_errmsg(conn_id: i64) -> *const c_char {
    ffi_guard(static_cstr(b"\0"), || {
        let msg = {
            let guard = lock_recover(conns());
            match guard.get(&conn_id) {
                #[cfg(feature = "dblib")]
                Some(Conn::Dblib(c)) => c.errmsg().to_string(),
                #[cfg(feature = "firebird")]
                Some(Conn::Firebird(c)) => c.errmsg().to_string(),
                #[cfg(any(feature = "odbc", feature = "informix"))]
                Some(Conn::Odbc(c)) => c.errmsg().to_string(),
                #[cfg(feature = "oci")]
                Some(Conn::Oci(c)) => c.errmsg().to_string(),
                Some(Conn::Sqlite(c)) => c.errmsg(),
                Some(Conn::Postgres(c)) => c.errmsg.clone(),
                Some(Conn::Mysql(c)) => c.errmsg.clone(),
                None => String::new(),
            }
        };
        store_cstr(errmsg_cell(), &msg)
    })
}

/// Returns a pointer to the 5-char SQLSTATE for the connection's last operation
/// (`"00000"` on success). Unknown handles also report `"00000"` (no operation
/// has been recorded for them), and so does a caught panic — the call that panicked
/// has already reported its own `-1`/`0` failure, which is what the prelude raises
/// on. Valid until the next `elephc_pdo_sqlstate`.
#[no_mangle]
pub extern "C" fn elephc_pdo_sqlstate(conn_id: i64) -> *const c_char {
    ffi_guard(static_cstr(b"00000\0"), || {
        let state = {
            let guard = lock_recover(conns());
            match guard.get(&conn_id) {
                #[cfg(feature = "dblib")]
                Some(Conn::Dblib(c)) => c.sqlstate().to_string(),
                #[cfg(feature = "firebird")]
                Some(Conn::Firebird(c)) => c.sqlstate().to_string(),
                #[cfg(any(feature = "odbc", feature = "informix"))]
                Some(Conn::Odbc(c)) => c.sqlstate().to_string(),
                #[cfg(feature = "oci")]
                Some(Conn::Oci(c)) => c.sqlstate().to_string(),
                Some(Conn::Sqlite(c)) => c.sqlstate(),
                Some(Conn::Postgres(c)) => c.sqlstate.clone(),
                Some(Conn::Mysql(c)) => c.sqlstate.clone(),
                None => "00000".to_string(),
            }
        };
        store_cstr(sqlstate_cell(), &state)
    })
}

/// Sets the busy-wait timeout (in milliseconds) for lock contention: SQLite calls
/// `sqlite3_busy_timeout`; PostgreSQL/MySQL have no equivalent client-side knob
/// for this bridge's one-statement-at-a-time connections, so they no-op and
/// report success. Returns `1`/`0`; a caught panic reports the `0` failure sentinel.
#[no_mangle]
pub extern "C" fn elephc_pdo_set_busy_timeout(conn_id: i64, ms: i64) -> i64 {
    ffi_guard(0, || {
        let guard = lock_recover(conns());
        match guard.get(&conn_id) {
            #[cfg(feature = "dblib")]
            Some(Conn::Dblib(_)) => 1,
            #[cfg(feature = "firebird")]
            Some(Conn::Firebird(_)) => 1,
            #[cfg(any(feature = "odbc", feature = "informix"))]
            Some(Conn::Odbc(_)) => 1,
            #[cfg(feature = "oci")]
            Some(Conn::Oci(_)) => 1,
            Some(Conn::Sqlite(c)) => c.set_busy_timeout(ms),
            Some(Conn::Postgres(_)) => 1,
            Some(Conn::Mysql(_)) => 1,
            None => 0,
        }
    })
}

/// Applies a writable PDO_DBLIB driver attribute. Returns `1` when DBLIB accepts
/// it and `0` for a read-only/unknown attribute, another driver, or a caught panic.
#[no_mangle]
pub extern "C" fn elephc_pdo_dblib_set_attribute(
    conn_id: i64,
    attribute: i64,
    value: i64,
) -> i64 {
    ffi_guard(0, || {
        #[cfg(feature = "dblib")]
        if let Some(Conn::Dblib(connection)) = lock_recover(conns()).get_mut(&conn_id) {
            return connection.set_attribute(attribute, value) as i64;
        }
        let _ = (conn_id, attribute, value);
        0
    })
}

/// Reads a boolean PDO_DBLIB driver attribute. Returns `0`/`1`, or `-1` when
/// the attribute is not readable, the handle uses another driver, or a panic occurs.
#[no_mangle]
pub extern "C" fn elephc_pdo_dblib_attribute_bool(conn_id: i64, attribute: i64) -> i64 {
    ffi_guard(-1, || {
        #[cfg(feature = "dblib")]
        if let Some(Conn::Dblib(connection)) = lock_recover(conns()).get(&conn_id) {
            return connection.attribute_bool(attribute).map_or(-1, i64::from);
        }
        let _ = (conn_id, attribute);
        -1
    })
}

/// Returns PDO_DBLIB's operating-system error code for connection errorInfo.
#[no_mangle]
pub extern "C" fn elephc_pdo_dblib_os_errcode(conn_id: i64) -> i64 {
    ffi_guard(0, || {
        #[cfg(feature = "dblib")]
        if let Some(Conn::Dblib(connection)) = lock_recover(conns()).get(&conn_id) {
            return connection.os_errcode();
        }
        let _ = conn_id;
        0
    })
}

/// Returns PDO_DBLIB's error severity for connection errorInfo.
#[no_mangle]
pub extern "C" fn elephc_pdo_dblib_severity(conn_id: i64) -> i64 {
    ffi_guard(0, || {
        #[cfg(feature = "dblib")]
        if let Some(Conn::Dblib(connection)) = lock_recover(conns()).get(&conn_id) {
            return connection.severity();
        }
        let _ = conn_id;
        0
    })
}

/// Returns PDO_DBLIB's operating-system diagnostic for a connection. The pointer
/// remains valid until the next call; unsupported or unknown handles return empty.
#[no_mangle]
pub extern "C" fn elephc_pdo_dblib_os_errmsg(conn_id: i64) -> *const c_char {
    ffi_guard(static_cstr(b"\0"), || {
        let message = {
            let guard = lock_recover(conns());
            match guard.get(&conn_id) {
                #[cfg(feature = "dblib")]
                Some(Conn::Dblib(connection)) => connection.os_errmsg().to_string(),
                _ => String::new(),
            }
        };
        store_cstr(dblib_os_errmsg_cell(), &message)
    })
}

/// Applies an integer or boolean PDO_FIREBIRD connection attribute.
#[no_mangle]
pub extern "C" fn elephc_pdo_firebird_set_attribute_int(
    conn_id: i64,
    attribute: i64,
    value: i64,
) -> i64 {
    ffi_guard(0, || {
        #[cfg(feature = "firebird")]
        if let Some(Conn::Firebird(connection)) = lock_recover(conns()).get_mut(&conn_id) {
            return connection.set_attribute_int(attribute, value) as i64;
        }
        let _ = (conn_id, attribute, value);
        0
    })
}

/// Applies a textual PDO_FIREBIRD date/time formatting attribute.
///
/// # Safety
/// `value` must point to a NUL-terminated string valid for the duration of the call.
#[no_mangle]
pub unsafe extern "C" fn elephc_pdo_firebird_set_attribute_text(
    conn_id: i64,
    attribute: i64,
    value: *const c_char,
) -> i64 {
    ffi_guard(0, || {
        #[cfg(feature = "firebird")]
        if let (Some(value), Some(Conn::Firebird(connection))) =
            (cstr_arg(value), lock_recover(conns()).get_mut(&conn_id))
        {
            return connection.set_attribute_text(attribute, value.to_string()) as i64;
        }
        let _ = (conn_id, attribute, value);
        0
    })
}

/// Reads an integer or boolean PDO_FIREBIRD connection attribute, or `-1` when
/// the attribute/handle is unsupported.
#[no_mangle]
pub extern "C" fn elephc_pdo_firebird_attribute_int(conn_id: i64, attribute: i64) -> i64 {
    ffi_guard(-1, || {
        #[cfg(feature = "firebird")]
        if let Some(Conn::Firebird(connection)) = lock_recover(conns()).get(&conn_id) {
            return connection.attribute_int(attribute).unwrap_or(-1);
        }
        let _ = (conn_id, attribute);
        -1
    })
}

/// Reads a textual PDO_FIREBIRD date/time formatting attribute. Unsupported
/// attributes and handles return an empty string.
#[no_mangle]
pub extern "C" fn elephc_pdo_firebird_attribute_text(
    conn_id: i64,
    attribute: i64,
) -> *const c_char {
    ffi_guard(static_cstr(b"\0"), || {
        let value = {
            #[cfg(feature = "firebird")]
            if let Some(Conn::Firebird(connection)) = lock_recover(conns()).get(&conn_id) {
                connection.attribute_text(attribute).unwrap_or_default().to_string()
            } else {
                String::new()
            }
            #[cfg(not(feature = "firebird"))]
            String::new()
        };
        let _ = (conn_id, attribute);
        store_cstr(firebird_attribute_cell(), &value)
    })
}

/// Returns the PDO parameter type reported by PDO_FIREBIRD `getColumnMeta()` for
/// one result column, or `2` (`PDO::PARAM_STR`) for unsupported input.
#[no_mangle]
pub extern "C" fn elephc_pdo_firebird_column_pdo_type(stmt_id: i64, column: i64) -> i64 {
    ffi_guard(2, || {
        #[cfg(feature = "firebird")]
        if let Some(Stmt::Firebird(statement)) = lock_recover(stmts()).get(&stmt_id) {
            return statement.column_pdo_type(column);
        }
        let _ = (stmt_id, column);
        2
    })
}

/// Stores PDO_FIREBIRD's statement cursor name after enforcing libfbclient's
/// 31-byte limit.
///
/// # Safety
/// `name` must point to a NUL-terminated string valid for the duration of the call.
#[no_mangle]
pub unsafe extern "C" fn elephc_pdo_firebird_stmt_set_cursor_name(
    stmt_id: i64,
    name: *const c_char,
) -> i64 {
    ffi_guard(0, || {
        #[cfg(feature = "firebird")]
        if let (Some(name), Some(Stmt::Firebird(statement))) =
            (cstr_arg(name), lock_recover(stmts()).get_mut(&stmt_id))
        {
            return statement.set_cursor_name(name.to_string()) as i64;
        }
        let _ = (stmt_id, name);
        0
    })
}

/// Returns PDO_FIREBIRD's configured statement cursor name, or an empty string
/// when no cursor name is set or the handle belongs to another driver.
#[no_mangle]
pub extern "C" fn elephc_pdo_firebird_stmt_cursor_name(stmt_id: i64) -> *const c_char {
    ffi_guard(static_cstr(b"\0"), || {
        let name = {
            #[cfg(feature = "firebird")]
            if let Some(Stmt::Firebird(statement)) = lock_recover(stmts()).get(&stmt_id) {
                statement.cursor_name().unwrap_or_default().to_string()
            } else {
                String::new()
            }
            #[cfg(not(feature = "firebird"))]
            String::new()
        };
        let _ = stmt_id;
        store_cstr(firebird_attribute_cell(), &name)
    })
}

/// Applies PDO_ODBC's writable connection attributes.
#[no_mangle]
pub extern "C" fn elephc_pdo_odbc_set_attribute(
    conn_id: i64,
    attribute: i64,
    value: i64,
) -> i64 {
    ffi_guard(0, || {
        #[cfg(any(feature = "odbc", feature = "informix"))]
        if let Some(Conn::Odbc(connection)) = lock_recover(conns()).get_mut(&conn_id) {
            return connection.set_attribute(attribute, value) as i64;
        }
        let _ = (conn_id, attribute, value);
        0
    })
}

/// Reads a PDO_ODBC boolean connection attribute, or `-1` when unsupported.
#[no_mangle]
pub extern "C" fn elephc_pdo_odbc_attribute(conn_id: i64, attribute: i64) -> i64 {
    ffi_guard(-1, || {
        #[cfg(any(feature = "odbc", feature = "informix"))]
        if let Some(Conn::Odbc(connection)) = lock_recover(conns()).get(&conn_id) {
            return connection.attribute(attribute).unwrap_or(-1);
        }
        let _ = (conn_id, attribute);
        -1
    })
}

/// Assigns the native cursor name for one PDO_ODBC statement.
///
/// # Safety
/// `name` must point to a NUL-terminated string valid for the duration of the call.
#[no_mangle]
pub unsafe extern "C" fn elephc_pdo_odbc_stmt_set_cursor_name(
    stmt_id: i64,
    name: *const c_char,
) -> i64 {
    ffi_guard(0, || {
        #[cfg(any(feature = "odbc", feature = "informix"))]
        if let (Some(name), Some(Stmt::Odbc(statement))) =
            (cstr_arg(name), lock_recover(stmts()).get_mut(&stmt_id))
        {
            return statement.set_cursor_name(name) as i64;
        }
        let _ = (stmt_id, name);
        0
    })
}

/// Returns the native cursor name for one PDO_ODBC statement.
#[no_mangle]
pub extern "C" fn elephc_pdo_odbc_stmt_cursor_name(stmt_id: i64) -> *const c_char {
    ffi_guard(static_cstr(b"\0"), || {
        let name = {
            #[cfg(any(feature = "odbc", feature = "informix"))]
            if let Some(Stmt::Odbc(statement)) = lock_recover(stmts()).get_mut(&stmt_id) {
                statement.cursor_name()
            } else {
                String::new()
            }
            #[cfg(not(any(feature = "odbc", feature = "informix")))]
            String::new()
        };
        let _ = stmt_id;
        store_cstr(firebird_attribute_cell(), &name)
    })
}

/// Mirrors php-src's statement-level ODBC UTF-8 setter return contract.
#[no_mangle]
pub extern "C" fn elephc_pdo_odbc_stmt_set_assume_utf8(stmt_id: i64, enabled: i64) -> i64 {
    ffi_guard(0, || {
        #[cfg(any(feature = "odbc", feature = "informix"))]
        if let Some(Stmt::Odbc(statement)) = lock_recover(stmts()).get_mut(&stmt_id) {
            return statement.set_assume_utf8(enabled != 0) as i64;
        }
        let _ = (stmt_id, enabled);
        0
    })
}

/// Mirrors php-src's statement-level ODBC UTF-8 getter return contract.
#[no_mangle]
pub extern "C" fn elephc_pdo_odbc_stmt_assume_utf8(stmt_id: i64) -> i64 {
    ffi_guard(0, || {
        #[cfg(any(feature = "odbc", feature = "informix"))]
        if let Some(Stmt::Odbc(statement)) = lock_recover(stmts()).get(&stmt_id) {
            return statement.assume_utf8() as i64;
        }
        let _ = stmt_id;
        0
    })
}

/// Applies an integer-valued PDO_OCI connection attribute.
#[no_mangle]
pub extern "C" fn elephc_pdo_oci_set_attribute_int(
    conn_id: i64,
    attribute: i64,
    value: i64,
) -> i64 {
    ffi_guard(0, || {
        #[cfg(feature = "oci")]
        if let Some(Conn::Oci(connection)) = lock_recover(conns()).get_mut(&conn_id) {
            return connection.set_attribute_int(attribute, value) as i64;
        }
        let _ = (conn_id, attribute, value);
        0
    })
}

/// Applies a text-valued PDO_OCI session attribute.
///
/// # Safety
/// `value` must point to a NUL-terminated string valid for the duration of the call.
#[no_mangle]
pub unsafe extern "C" fn elephc_pdo_oci_set_attribute_text(
    conn_id: i64,
    attribute: i64,
    value: *const c_char,
) -> i64 {
    ffi_guard(0, || {
        #[cfg(feature = "oci")]
        if let (Some(value), Some(Conn::Oci(connection))) =
            (cstr_arg(value), lock_recover(conns()).get_mut(&conn_id))
        {
            return connection.set_attribute_text(attribute, value) as i64;
        }
        let _ = (conn_id, attribute, value);
        0
    })
}

/// Reads an integer-valued PDO_OCI connection attribute, or `-1` if unsupported.
#[no_mangle]
pub extern "C" fn elephc_pdo_oci_attribute_int(conn_id: i64, attribute: i64) -> i64 {
    ffi_guard(-1, || {
        #[cfg(feature = "oci")]
        if let Some(Conn::Oci(connection)) = lock_recover(conns()).get_mut(&conn_id) {
            return connection.attribute_int(attribute).unwrap_or(-1);
        }
        let _ = (conn_id, attribute);
        -1
    })
}

/// Returns PDO_OCI's parameter type for one result column.
#[no_mangle]
pub extern "C" fn elephc_pdo_oci_column_pdo_type(stmt_id: i64, column: i64) -> i64 {
    ffi_guard(2, || {
        #[cfg(feature = "oci")]
        if let Some(Stmt::Oci(statement)) = lock_recover(stmts()).get(&stmt_id) {
            return statement.column_pdo_type(column);
        }
        let _ = (stmt_id, column);
        2
    })
}

/// Returns PDO_OCI's numeric scale for one result column.
#[no_mangle]
pub extern "C" fn elephc_pdo_oci_column_scale(stmt_id: i64, column: i64) -> i64 {
    ffi_guard(0, || {
        #[cfg(feature = "oci")]
        if let Some(Stmt::Oci(statement)) = lock_recover(stmts()).get(&stmt_id) {
            return statement.column_scale(column);
        }
        let _ = (stmt_id, column);
        0
    })
}

/// Returns PDO_OCI's nullable/not-null/blob metadata flag bits.
#[no_mangle]
pub extern "C" fn elephc_pdo_oci_column_flags(stmt_id: i64, column: i64) -> i64 {
    ffi_guard(0, || {
        #[cfg(feature = "oci")]
        if let Some(Stmt::Oci(statement)) = lock_recover(stmts()).get(&stmt_id) {
            return statement.column_flags(column);
        }
        let _ = (stmt_id, column);
        0
    })
}

/// Enables or disables SQLite's extended result codes for a `sqlite:` connection
/// (`sqlite3_extended_result_codes`), backing
/// `Pdo\Sqlite::ATTR_EXTENDED_RESULT_CODES` (1002, F-SQLT-02): with it on,
/// `errorInfo()[1]` reports the specific code (e.g. 2067 `SQLITE_CONSTRAINT_UNIQUE`)
/// instead of the primary one (19 `SQLITE_CONSTRAINT`). Returns `1` on success, `0`
/// for a non-SQLite connection, an unknown handle, or a caught panic.
#[no_mangle]
pub extern "C" fn elephc_pdo_set_extended_result_codes(conn_id: i64, on: i64) -> i64 {
    ffi_guard(0, || {
        let guard = lock_recover(conns());
        match guard.get(&conn_id) {
            Some(Conn::Sqlite(c)) => c.set_extended_result_codes(on),
            _ => 0,
        }
    })
}

/// Stores the PHP 8.5 SQLite transaction mode for future `beginTransaction()` calls.
#[no_mangle]
pub extern "C" fn elephc_pdo_set_transaction_mode(conn_id: i64, mode: i64) -> i64 {
    ffi_guard(0, || match lock_recover(conns()).get(&conn_id) {
        Some(Conn::Sqlite(c)) => c.set_transaction_mode(mode),
        _ => 0,
    })
}

/// Returns the PHP 8.5 SQLite transaction mode, or `-1` for another driver/handle.
#[no_mangle]
pub extern "C" fn elephc_pdo_transaction_mode(conn_id: i64) -> i64 {
    ffi_guard(-1, || match lock_recover(conns()).get(&conn_id) {
        Some(Conn::Sqlite(c)) => c.transaction_mode(),
        _ => -1,
    })
}

/// Returns a pointer to the connection's server/library version string: SQLite's
/// bundled `sqlite3_libversion()`, or the PostgreSQL/MySQL server's reported
/// version. Empty for an unknown handle — and for a caught panic. Valid until the
/// next `elephc_pdo_server_version`.
#[no_mangle]
pub extern "C" fn elephc_pdo_server_version(conn_id: i64) -> *const c_char {
    ffi_guard(static_cstr(b"\0"), || {
        let version = {
            let mut guard = lock_recover(conns());
            match guard.get_mut(&conn_id) {
                #[cfg(feature = "dblib")]
                Some(Conn::Dblib(c)) => c.tds_version().to_string(),
                #[cfg(feature = "firebird")]
                Some(Conn::Firebird(c)) => c.server_version(),
                #[cfg(any(feature = "odbc", feature = "informix"))]
                Some(Conn::Odbc(c)) => c.server_version(),
                #[cfg(feature = "oci")]
                Some(Conn::Oci(c)) => c.server_version(),
                Some(Conn::Sqlite(c)) => c.server_version(),
                Some(Conn::Postgres(c)) => c.server_version(),
                Some(Conn::Mysql(c)) => c.server_version(),
                None => String::new(),
            }
        };
        store_cstr(server_version_cell(), &version)
    })
}

/// Returns the connection driver's linked client implementation/version string.
/// SQLite reports the embedded SQLite version exactly like php-src; PostgreSQL and
/// MySQL report their statically linked pure-Rust client crate. Empty for an
/// unknown handle or caught panic. Valid until the next call to this function.
#[no_mangle]
pub extern "C" fn elephc_pdo_client_version(conn_id: i64) -> *const c_char {
    ffi_guard(static_cstr(b"\0"), || {
        let version = {
            let guard = lock_recover(conns());
            match guard.get(&conn_id) {
                #[cfg(feature = "dblib")]
                Some(Conn::Dblib(c)) => c.client_version(),
                #[cfg(feature = "firebird")]
                Some(Conn::Firebird(c)) => c.client_version(),
                #[cfg(any(feature = "odbc", feature = "informix"))]
                Some(Conn::Odbc(c)) => c.client_version(),
                #[cfg(feature = "oci")]
                Some(Conn::Oci(c)) => c.client_version(),
                Some(Conn::Sqlite(c)) => c.client_version(),
                Some(Conn::Postgres(c)) => c.client_version(),
                Some(Conn::Mysql(c)) => c.client_version(),
                None => String::new(),
            }
        };
        store_cstr(client_version_cell(), &version)
    })
}

/// Returns the live driver server-information text. SQLite does not implement
/// `PDO::ATTR_SERVER_INFO`, so it returns an empty string for the prelude to route
/// to IM001. Empty also represents an unknown handle, query failure, or panic.
/// Valid until the next call to this function.
#[no_mangle]
pub extern "C" fn elephc_pdo_server_info(conn_id: i64) -> *const c_char {
    ffi_guard(static_cstr(b"\0"), || {
        let info = {
            let mut guard = lock_recover(conns());
            match guard.get_mut(&conn_id) {
                #[cfg(feature = "dblib")]
                Some(Conn::Dblib(_)) => String::new(),
                #[cfg(feature = "firebird")]
                Some(Conn::Firebird(c)) => c.server_version(),
                #[cfg(any(feature = "odbc", feature = "informix"))]
                Some(Conn::Odbc(c)) => c.server_info(),
                #[cfg(feature = "oci")]
                Some(Conn::Oci(c)) => c.server_info(),
                Some(Conn::Postgres(c)) => c.server_info(),
                Some(Conn::Mysql(c)) => c.server_info(),
                Some(Conn::Sqlite(_)) | None => String::new(),
            }
        };
        store_cstr(server_info_cell(), &info)
    })
}

/// Returns the driver's connection-status text. PostgreSQL maps its live closed
/// state to libpq's status strings; MySQL returns its resolved transport description.
/// SQLite does not implement the attribute and returns empty. Valid until the next
/// call to this function; empty also covers an unknown handle or caught panic.
#[no_mangle]
pub extern "C" fn elephc_pdo_connection_status(conn_id: i64) -> *const c_char {
    ffi_guard(static_cstr(b"\0"), || {
        let status = {
            let mut guard = lock_recover(conns());
            match guard.get_mut(&conn_id) {
                #[cfg(feature = "dblib")]
                Some(Conn::Dblib(c)) => {
                    if c.is_alive() { "Connection OK".to_string() } else { "Connection failed".to_string() }
                }
                #[cfg(feature = "firebird")]
                Some(Conn::Firebird(c)) => c.connection_status(),
                #[cfg(any(feature = "odbc", feature = "informix"))]
                Some(Conn::Odbc(_)) => String::new(),
                #[cfg(feature = "oci")]
                Some(Conn::Oci(_)) => String::new(),
                Some(Conn::Postgres(c)) => c.connection_status(),
                Some(Conn::Mysql(c)) => c.connection_status(),
                Some(Conn::Sqlite(_)) | None => String::new(),
            }
        };
        store_cstr(connection_status_cell(), &status)
    })
}

/// Returns the PostgreSQL backend process id for a `pgsql:` connection (backs
/// `Pdo\Pgsql::getPid()`); 0 for a SQLite/MySQL connection, an unknown handle, or a
/// caught panic.
#[no_mangle]
pub extern "C" fn elephc_pdo_backend_pid(conn_id: i64) -> i64 {
    ffi_guard(0, || {
        let mut guard = lock_recover(conns());
        match guard.get_mut(&conn_id) {
            #[cfg(feature = "dblib")]
            Some(Conn::Dblib(_)) => 0,
            #[cfg(feature = "firebird")]
            Some(Conn::Firebird(_)) => 0,
            #[cfg(any(feature = "odbc", feature = "informix"))]
            Some(Conn::Odbc(_)) => 0,
            #[cfg(feature = "oci")]
            Some(Conn::Oci(_)) => 0,
            Some(Conn::Postgres(c)) => c.backend_pid(),
            Some(Conn::Sqlite(_)) => 0,
            Some(Conn::Mysql(_)) => 0,
            None => 0,
        }
    })
}

/// Returns the number of warnings from the last statement on a `mysql:` connection
/// (backs `Pdo\Mysql::getWarningCount()`); 0 for a SQLite/PostgreSQL connection, an
/// unknown handle, or a caught panic.
#[no_mangle]
pub extern "C" fn elephc_pdo_warning_count(conn_id: i64) -> i64 {
    ffi_guard(0, || {
        let mut guard = lock_recover(conns());
        match guard.get_mut(&conn_id) {
            #[cfg(feature = "dblib")]
            Some(Conn::Dblib(_)) => 0,
            #[cfg(feature = "firebird")]
            Some(Conn::Firebird(_)) => 0,
            #[cfg(any(feature = "odbc", feature = "informix"))]
            Some(Conn::Odbc(_)) => 0,
            #[cfg(feature = "oci")]
            Some(Conn::Oci(_)) => 0,
            Some(Conn::Mysql(c)) => c.warning_count(),
            Some(Conn::Sqlite(_)) => 0,
            Some(Conn::Postgres(_)) => 0,
            None => 0,
        }
    })
}

/// Returns `1` when a `mysql:` connection's session has `NO_BACKSLASH_ESCAPES`
/// active in its `sql_mode` (backslash is then a literal character in a string
/// literal, so `PDO::quote()`'s usual backslash-escaping is unsafe there and
/// must fall back to `''`-doubling only — P1-f); `0` for a SQLite/PostgreSQL
/// connection, an unknown handle, or a caught panic (`0` = "escape as usual", the
/// conservative answer, since the connection's real `sql_mode` could not be read).
#[no_mangle]
pub extern "C" fn elephc_pdo_no_backslash_escapes(conn_id: i64) -> i64 {
    ffi_guard(0, || {
        let guard = lock_recover(conns());
        match guard.get(&conn_id) {
            Some(Conn::Mysql(c)) => c.no_backslash_escape() as i64,
            _ => 0,
        }
    })
}

/// Creates a large object and returns its OID as text for a `pgsql:` connection
/// (`Pdo\Pgsql::lobCreate()`); empty string for a non-PostgreSQL connection, an
/// unknown handle, an error, or a caught panic.
#[no_mangle]
pub extern "C" fn elephc_pdo_lob_create(conn_id: i64) -> *const c_char {
    ffi_guard(static_cstr(b"\0"), || {
        let text = {
            let mut guard = lock_recover(conns());
            match guard.get_mut(&conn_id) {
                Some(Conn::Postgres(c)) => c.lob_create(),
                _ => String::new(),
            }
        };
        store_cstr(pg_text_result_cell(), &text)
    })
}

/// Deletes a large object by OID for a `pgsql:` connection (`Pdo\Pgsql::lobUnlink()`);
/// returns 1 on success, 0 for a non-PostgreSQL connection, unknown handle, error, or
/// a caught panic.
///
/// # Safety
/// `oid` must point to a NUL-terminated string valid for the call.
#[no_mangle]
pub unsafe extern "C" fn elephc_pdo_lob_unlink(conn_id: i64, oid: *const c_char) -> i64 {
    ffi_guard(0, || {
        let Some(oid) = cstr_arg(oid) else {
            return 0;
        };
        let mut guard = lock_recover(conns());
        match guard.get_mut(&conn_id) {
            Some(Conn::Postgres(c)) => c.lob_unlink(oid),
            _ => 0,
        }
    })
}

/// Runs a prelude-built `COPY … FROM STDIN` for a `pgsql:` connection, streaming
/// `data` into it (`Pdo\Pgsql::copyFromArray()` / `copyFromFile()`); returns the row
/// count copied, or -1 for a non-PostgreSQL connection, unknown handle, error, or a
/// caught panic.
///
/// # Safety
/// `copy_sql` and `data` must point to NUL-terminated strings valid for the call.
#[no_mangle]
pub unsafe extern "C" fn elephc_pdo_copy_in(
    conn_id: i64,
    copy_sql: *const c_char,
    data: *const c_char,
) -> i64 {
    ffi_guard(-1, || {
        let (Some(sql), Some(data)) = (cstr_arg(copy_sql), cstr_arg(data)) else {
            return -1;
        };
        let mut guard = lock_recover(conns());
        match guard.get_mut(&conn_id) {
            Some(Conn::Postgres(c)) => c.copy_in(sql, data.as_bytes()),
            _ => -1,
        }
    })
}

/// Runs a prelude-built `COPY … TO STDOUT` for a `pgsql:` connection and returns the
/// raw text output (`Pdo\Pgsql::copyToArray()` / `copyToFile()`); empty string for a
/// non-PostgreSQL connection, unknown handle, or error. P2-i: a caller cannot tell
/// "really empty" apart from "error" by this return value alone — `copy_out` always
/// resets `elephc_pdo_errcode()` to `0` on success (even an empty one) and sets it
/// non-zero on error, so the prelude checks that accessor right after this call to
/// make the distinction, satisfying `copyToArray()`'s `array|false` contract. A
/// caught panic degrades to that same empty string.
///
/// # Safety
/// `copy_sql` must point to a NUL-terminated string valid for the call.
#[no_mangle]
pub unsafe extern "C" fn elephc_pdo_copy_out(
    conn_id: i64,
    copy_sql: *const c_char,
) -> *const c_char {
    ffi_guard(static_cstr(b"\0"), || {
        let Some(sql) = cstr_arg(copy_sql) else {
            return store_cstr(pg_text_result_cell(), "");
        };
        let text = {
            let mut guard = lock_recover(conns());
            match guard.get_mut(&conn_id) {
                Some(Conn::Postgres(c)) => c.copy_out(sql),
                _ => String::new(),
            }
        };
        store_cstr(pg_text_result_cell(), &text)
    })
}

/// Polls a `pgsql:` connection for a pending LISTEN/NOTIFY notification
/// (`Pdo\Pgsql::getNotify()`), returning it as `channel\tpid\tpayload`, or an empty
/// string if none arrives within `timeout_ms` (or for a non-PostgreSQL connection /
/// unknown handle / a caught panic).
#[no_mangle]
pub extern "C" fn elephc_pdo_get_notify(conn_id: i64, timeout_ms: i64) -> *const c_char {
    ffi_guard(static_cstr(b"\0"), || {
        let text = {
            let mut guard = lock_recover(conns());
            match guard.get_mut(&conn_id) {
                Some(Conn::Postgres(c)) => c.get_notify(timeout_ms),
                _ => String::new(),
            }
        };
        store_cstr(pg_text_result_cell(), &text)
    })
}

/// Drains one buffered server NOTICE message from a `pgsql:` connection
/// (`Pdo\Pgsql::setNoticeCallback()`), returning its text, or an empty string when
/// none is pending (or for a non-PostgreSQL connection / unknown handle). The prelude
/// calls this in a loop after each `exec()`/`query()` and dispatches each message to
/// the registered PHP callback. The returned pointer is valid until the next
/// PostgreSQL text-returning bridge call on this thread. A caught panic reports the
/// same empty string as "nothing pending", which simply ends the prelude's drain loop.
#[no_mangle]
pub extern "C" fn elephc_pdo_get_notice(conn_id: i64) -> *const c_char {
    ffi_guard(static_cstr(b"\0"), || {
        let text = {
            let guard = lock_recover(conns());
            match guard.get(&conn_id) {
                Some(Conn::Postgres(c)) => c.drain_notice(),
                _ => String::new(),
            }
        };
        store_cstr(pg_text_result_cell(), &text)
    })
}

/// Reads a SQLite BLOB cell whole into the shared blob buffer (`Pdo\Sqlite::openBlob()`),
/// returning its length in bytes, or -1 for a non-SQLite connection, an unknown handle,
/// a read error (missing row/column), or a caught panic. The bytes are then copied out
/// in one shot with `elephc_pdo_blob_data_ptr` (or, on the fallback path, drained with
/// `elephc_pdo_blob_byte`); both preserve embedded NUL bytes.
///
/// # Safety
/// `table`, `column`, and `dbname` must point to NUL-terminated strings valid for the
/// call (`dbname` may be null, treated as `"main"`).
#[no_mangle]
pub unsafe extern "C" fn elephc_pdo_blob_read(
    conn_id: i64,
    table: *const c_char,
    column: *const c_char,
    rowid: i64,
    dbname: *const c_char,
) -> i64 {
    ffi_guard(-1, || {
        let (Some(table), Some(column)) = (cstr_arg(table), cstr_arg(column)) else {
            return -1;
        };
        let dbname = cstr_arg(dbname).unwrap_or("main");
        let result = {
            let mut guard = lock_recover(conns());
            match guard.get_mut(&conn_id) {
                Some(Conn::Sqlite(c)) => c.blob_read(dbname, table, column, rowid),
                _ => return -1,
            }
        };
        match result {
            Ok(bytes) => {
                let len = bytes.len() as i64;
                *lock_recover(blob_cell()) = bytes;
                len
            }
            Err(_) => -1,
        }
    })
}

/// Returns a SQLite BLOB cell's fixed byte size without transferring its data,
/// or `-1` for invalid input, an unknown/non-SQLite handle, or a SQLite failure.
///
/// # Safety
/// `table`, `column`, and `dbname` must point to NUL-terminated strings valid for
/// the call (`dbname` may be null, treated as `"main"`).
#[no_mangle]
pub unsafe extern "C" fn elephc_pdo_blob_size(
    conn_id: i64,
    table: *const c_char,
    column: *const c_char,
    rowid: i64,
    dbname: *const c_char,
) -> i64 {
    ffi_guard(-1, || {
        let (Some(table), Some(column)) = (cstr_arg(table), cstr_arg(column)) else {
            return -1;
        };
        let dbname = cstr_arg(dbname).unwrap_or("main");
        let guard = lock_recover(conns());
        match guard.get(&conn_id) {
            Some(Conn::Sqlite(connection)) => connection
                .blob_size(dbname, table, column, rowid)
                .unwrap_or(-1),
            _ => -1,
        }
    })
}

/// Reads one bounded SQLite BLOB slice into the shared binary buffer and returns
/// its length, or `-1` for invalid input, a bad handle, or a SQLite failure.
///
/// # Safety
/// `table`, `column`, and `dbname` must point to NUL-terminated strings valid for
/// the call (`dbname` may be null, treated as `"main"`).
#[no_mangle]
pub unsafe extern "C" fn elephc_pdo_blob_read_at(
    conn_id: i64,
    table: *const c_char,
    column: *const c_char,
    rowid: i64,
    dbname: *const c_char,
    offset: i64,
    length: i64,
) -> i64 {
    ffi_guard(-1, || {
        let (Some(table), Some(column)) = (cstr_arg(table), cstr_arg(column)) else {
            return -1;
        };
        let dbname = cstr_arg(dbname).unwrap_or("main");
        let result = {
            let guard = lock_recover(conns());
            match guard.get(&conn_id) {
                Some(Conn::Sqlite(connection)) => {
                    connection.blob_read_at(dbname, table, column, rowid, offset, length)
                }
                _ => return -1,
            }
        };
        match result {
            Ok(bytes) => {
                let length = bytes.len() as i64;
                *lock_recover(blob_cell()) = bytes;
                length
            }
            Err(_) => -1,
        }
    })
}

/// Writes one bounded slice of an existing fixed-size SQLite BLOB and returns the
/// bytes written, or `-1` when the range would extend it or another error occurs.
///
/// # Safety
/// String identifiers must be valid NUL-terminated strings for the call, and
/// `data` must expose at least `len` readable bytes (it may be null when `len` is 0).
#[no_mangle]
pub unsafe extern "C" fn elephc_pdo_blob_write_at(
    conn_id: i64,
    table: *const c_char,
    column: *const c_char,
    rowid: i64,
    dbname: *const c_char,
    offset: i64,
    data: *const c_char,
    len: i64,
) -> i64 {
    ffi_guard(-1, || {
        if len < 0 {
            return -1;
        }
        let (Some(table), Some(column)) = (cstr_arg(table), cstr_arg(column)) else {
            return -1;
        };
        let dbname = cstr_arg(dbname).unwrap_or("main");
        let bytes = bytes_arg(data, len);
        let guard = lock_recover(conns());
        match guard.get(&conn_id) {
            Some(Conn::Sqlite(connection)) => connection
                .blob_write_at(dbname, table, column, rowid, offset, &bytes)
                .unwrap_or(-1),
            _ => -1,
        }
    })
}

/// Reads a PostgreSQL large object whole into the shared blob buffer for pre-v45
/// ABI callers, returning its length in bytes, or -1 for a
/// non-PostgreSQL connection, an unknown handle, a non-numeric OID, a server error
/// (no such object), or a caught panic. The bytes are then copied out with
/// `elephc_pdo_blob_data_ptr` (or drained with `elephc_pdo_blob_byte`).
///
/// # Safety
/// `oid` must point to a NUL-terminated string valid for the call.
#[no_mangle]
pub unsafe extern "C" fn elephc_pdo_lob_get(conn_id: i64, oid: *const c_char) -> i64 {
    ffi_guard(-1, || {
        let Some(oid) = cstr_arg(oid) else {
            return -1;
        };
        let result = {
            let mut guard = lock_recover(conns());
            match guard.get_mut(&conn_id) {
                Some(Conn::Postgres(c)) => c.lob_get(oid),
                _ => return -1,
            }
        };
        match result {
            Some(bytes) => {
                let len = bytes.len() as i64;
                *lock_recover(blob_cell()) = bytes;
                len
            }
            None => -1,
        }
    })
}

/// Returns a PostgreSQL large object's current size without transferring its data.
///
/// # Safety
/// `oid` must point to a NUL-terminated string valid for the call.
#[no_mangle]
pub unsafe extern "C" fn elephc_pdo_lob_size(conn_id: i64, oid: *const c_char) -> i64 {
    ffi_guard(-1, || {
        let Some(oid) = cstr_arg(oid) else {
            return -1;
        };
        let mut guard = lock_recover(conns());
        match guard.get_mut(&conn_id) {
            Some(Conn::Postgres(connection)) => connection.lob_size(oid).unwrap_or(-1),
            _ => -1,
        }
    })
}

/// Reads one PostgreSQL large-object slice into the shared binary buffer and
/// returns its length, or `-1` on invalid input/server failure.
///
/// # Safety
/// `oid` must point to a NUL-terminated string valid for the call.
#[no_mangle]
pub unsafe extern "C" fn elephc_pdo_lob_read_at(
    conn_id: i64,
    oid: *const c_char,
    offset: i64,
    length: i64,
) -> i64 {
    ffi_guard(-1, || {
        let Some(oid) = cstr_arg(oid) else {
            return -1;
        };
        let result = {
            let mut guard = lock_recover(conns());
            match guard.get_mut(&conn_id) {
                Some(Conn::Postgres(connection)) => connection.lob_read_at(oid, offset, length),
                _ => return -1,
            }
        };
        match result {
            Some(bytes) => {
                let length = bytes.len() as i64;
                *lock_recover(blob_cell()) = bytes;
                length
            }
            None => -1,
        }
    })
}

/// Writes one PostgreSQL large-object slice at an explicit byte offset.
///
/// # Safety
/// `oid` must be a valid NUL-terminated string and `data` must expose at least
/// `len` readable bytes (it may be null when `len` is zero).
#[no_mangle]
pub unsafe extern "C" fn elephc_pdo_lob_write_at(
    conn_id: i64,
    oid: *const c_char,
    offset: i64,
    data: *const c_char,
    len: i64,
) -> i64 {
    ffi_guard(-1, || {
        let Some(oid) = cstr_arg(oid) else {
            return -1;
        };
        let bytes = bytes_arg(data, len);
        let mut guard = lock_recover(conns());
        match guard.get_mut(&conn_id) {
            Some(Conn::Postgres(connection)) => connection.lob_write_at(oid, offset, &bytes),
            _ => -1,
        }
    })
}

/// Writes the complete fixed-size snapshot of a SQLite BLOB back through
/// `sqlite3_blob_write`, returning 1 on success and 0 for a bad handle, invalid
/// identifiers, a size change, a SQLite error, or a caught panic.
///
/// # Safety
/// The string arguments must be valid NUL-terminated strings for the call, and
/// `data` must expose at least `len` readable bytes (it may be null when `len` is 0).
#[no_mangle]
pub unsafe extern "C" fn elephc_pdo_blob_write(
    conn_id: i64,
    table: *const c_char,
    column: *const c_char,
    rowid: i64,
    dbname: *const c_char,
    data: *const c_char,
    len: i64,
) -> i64 {
    ffi_guard(0, || {
        let (Some(table), Some(column)) = (cstr_arg(table), cstr_arg(column)) else {
            return 0;
        };
        let dbname = cstr_arg(dbname).unwrap_or("main");
        let bytes = bytes_arg(data, len);
        let guard = lock_recover(conns());
        match guard.get(&conn_id) {
            Some(Conn::Sqlite(c)) => c
                .blob_write(dbname, table, column, rowid, &bytes)
                .is_ok() as i64,
            _ => 0,
        }
    })
}

/// Writes a complete PostgreSQL large-object snapshot at offset zero with
/// `lo_put`, returning 1 on success and 0 for an invalid OID/handle, server error,
/// or caught panic. Embedded NUL bytes are preserved by the explicit byte length.
///
/// # Safety
/// `oid` must be a valid NUL-terminated string for the call, and `data` must expose
/// at least `len` readable bytes (it may be null when `len` is 0).
#[no_mangle]
pub unsafe extern "C" fn elephc_pdo_lob_put(
    conn_id: i64,
    oid: *const c_char,
    data: *const c_char,
    len: i64,
) -> i64 {
    ffi_guard(0, || {
        let Some(oid) = cstr_arg(oid) else {
            return 0;
        };
        let bytes = bytes_arg(data, len);
        let mut guard = lock_recover(conns());
        match guard.get_mut(&conn_id) {
            Some(Conn::Postgres(c)) => c.lob_put(oid, &bytes),
            _ => 0,
        }
    })
}

/// Returns a pointer to the first byte of the shared blob buffer filled by the most
/// recent `elephc_pdo_blob_read` / `elephc_pdo_lob_get`, or a NULL pointer when that
/// buffer is empty (which is also the caught-panic sentinel). Mirrors the
/// `elephc_pdo_column_data_ptr` contract: the pointer is valid until the next call
/// that rewrites the cell, and the prelude copies the whole run of
/// `elephc_pdo_blob_read`-reported bytes out immediately through `ptr_read_string`.
/// It exists so a BLOB is bulk-copied in one call instead of one PHP-level bridge
/// call per byte through `elephc_pdo_blob_byte` (kept as the fallback/compat path).
#[no_mangle]
pub extern "C" fn elephc_pdo_blob_data_ptr() -> *const c_char {
    ffi_guard(std::ptr::null(), || {
        let guard = lock_recover(blob_cell());
        if guard.is_empty() {
            std::ptr::null()
        } else {
            guard.as_ptr() as *const c_char
        }
    })
}

/// Returns the byte at `offset` in the shared blob buffer populated by the most recent
/// `elephc_pdo_blob_read` / `elephc_pdo_lob_get`, or 0 when out of range (or on a caught
/// panic). This is the fallback/compat drain path — one bridge call per byte — kept
/// alongside the bulk `elephc_pdo_blob_data_ptr`; like `elephc_pdo_column_data_byte`,
/// it preserves embedded NUL bytes on the round-trip into a PHP string.
#[no_mangle]
pub extern "C" fn elephc_pdo_blob_byte(offset: i64) -> i64 {
    ffi_guard(0, || {
        if offset < 0 {
            return 0;
        }
        let guard = lock_recover(blob_cell());
        guard.get(offset as usize).map(|&b| b as i64).unwrap_or(0)
    })
}

/// Prepares a statement (`PDO::prepare` / `PDO::query`) and returns an `i64`
/// statement handle, or `-1` on a compile error — the same sentinel a caught panic
/// degrades to.
///
/// # Safety
/// `sql` must point to a NUL-terminated string valid for the duration of the call.
#[no_mangle]
pub unsafe extern "C" fn elephc_pdo_prepare(
    conn_id: i64,
    sql: *const c_char,
    emulated: i64,
) -> i64 {
    ffi_guard(-1, || {
        let sqlite_db = match lock_recover(conns()).get(&conn_id) {
            Some(Conn::Sqlite(connection)) => Some(connection.db),
            _ => None,
        };
        let prepared: Result<Stmt, ()> = if let Some(db) = sqlite_db {
            sqlite::SqliteConn::prepare_on(db, sql).map(Stmt::Sqlite)
        } else {
            let mut guard = lock_recover(conns());
            match guard.get_mut(&conn_id) {
                #[cfg(feature = "dblib")]
                Some(Conn::Dblib(connection)) => match cstr_arg(sql) {
                    Some(sql) => match dblib::DblibStmt::new(conn_id, sql) {
                        Ok(statement) => Ok(Stmt::Dblib(statement)),
                        Err(message) => {
                            connection.set_error("HY093", 0, message);
                            Err(())
                        }
                    },
                    None => Err(()),
                },
                #[cfg(feature = "firebird")]
                Some(Conn::Firebird(connection)) => match cstr_arg(sql) {
                    Some(sql) => match firebird::FirebirdStmt::new(conn_id, sql) {
                        Ok(statement) => Ok(Stmt::Firebird(statement)),
                        Err(message) => {
                            connection.set_error("HY093", message);
                            Err(())
                        }
                    },
                    None => Err(()),
                },
                #[cfg(any(feature = "odbc", feature = "informix"))]
                Some(Conn::Odbc(connection)) => match cstr_arg(sql) {
                    Some(sql) => odbc::OdbcStmt::new(connection, conn_id, sql, emulated == 2)
                        .map(Stmt::Odbc)
                        .map_err(|_| ()),
                    None => Err(()),
                },
                #[cfg(feature = "oci")]
                Some(Conn::Oci(connection)) => match cstr_arg(sql) {
                    Some(sql) => oci::OciStmt::new(connection, conn_id, sql)
                        .map(Stmt::Oci)
                        .map_err(|_| ()),
                    None => Err(()),
                },
                Some(Conn::Postgres(c)) => match cstr_arg(sql) {
                    Some(s) => match c.prepare(s, emulated != 0) {
                        Ok(mut st) => {
                            st.conn_id = conn_id;
                            Ok(Stmt::Postgres(st))
                        }
                        Err(_) => Err(()),
                    },
                    None => Err(()),
                },
                Some(Conn::Mysql(c)) => match cstr_arg(sql) {
                    Some(s) => match c.prepare(s, emulated != 0) {
                        Ok(mut st) => {
                            st.conn_id = conn_id;
                            Ok(Stmt::Mysql(st))
                        }
                        Err(_) => Err(()),
                    },
                    None => Err(()),
                },
                Some(Conn::Sqlite(_)) | None => Err(()),
            }
        };
        match prepared {
            Ok(stmt) => {
                let id = next_id();
                lock_recover(stmts()).insert(id, stmt);
                id
            }
            Err(()) => -1,
        }
    })
}

/// Resolves a named placeholder to its 1-based bind index, or `0` when unknown (also
/// the caught-panic sentinel).
///
/// # Safety
/// `name` must point to a NUL-terminated string valid for the duration of the call.
#[no_mangle]
pub unsafe extern "C" fn elephc_pdo_bind_parameter_index(stmt_id: i64, name: *const c_char) -> i64 {
    ffi_guard(0, || {
        let guard = lock_recover(stmts());
        let Some(name) = cstr_arg(name) else {
            return 0;
        };
        match guard.get(&stmt_id) {
            #[cfg(feature = "dblib")]
            Some(Stmt::Dblib(s)) => s.parameter_index(name),
            #[cfg(feature = "firebird")]
            Some(Stmt::Firebird(s)) => s.parameter_index(name),
            #[cfg(any(feature = "odbc", feature = "informix"))]
            Some(Stmt::Odbc(s)) => s.parameter_index(name),
            #[cfg(feature = "oci")]
            Some(Stmt::Oci(s)) => s.parameter_index(name),
            Some(Stmt::Sqlite(s)) => s.bind_parameter_index(name),
            Some(Stmt::Postgres(s)) => s.bind_parameter_index(name),
            Some(Stmt::Mysql(s)) => s.bind_parameter_index(name),
            None => 0,
        }
    })
}

/// Binds an integer to the 1-based placeholder `idx`. Returns `1`/`0`; a caught panic
/// reports the `0` failure sentinel.
#[no_mangle]
pub extern "C" fn elephc_pdo_bind_int(stmt_id: i64, idx: i64, val: i64) -> i64 {
    ffi_guard(0, || {
        let mut guard = lock_recover(stmts());
        match guard.get_mut(&stmt_id) {
            #[cfg(feature = "dblib")]
            Some(Stmt::Dblib(s)) => s.bind_int(idx, val) as i64,
            #[cfg(feature = "firebird")]
            Some(Stmt::Firebird(s)) => s.bind_int(idx, val) as i64,
            #[cfg(any(feature = "odbc", feature = "informix"))]
            Some(Stmt::Odbc(s)) => s.bind_int(idx, val) as i64,
            #[cfg(feature = "oci")]
            Some(Stmt::Oci(s)) => s.bind_int(idx, val) as i64,
            Some(Stmt::Sqlite(s)) => s.bind_int(idx, val),
            Some(Stmt::Postgres(s)) => s.bind(idx, pg::Bind::Int(val)),
            Some(Stmt::Mysql(s)) => s.bind(idx, my::Bind::Int(val)),
            None => 0,
        }
    })
}

/// Binds a double to the 1-based placeholder `idx`. Returns `1`/`0`; a caught panic
/// reports the `0` failure sentinel.
#[no_mangle]
pub extern "C" fn elephc_pdo_bind_double(stmt_id: i64, idx: i64, val: f64) -> i64 {
    ffi_guard(0, || {
        let mut guard = lock_recover(stmts());
        match guard.get_mut(&stmt_id) {
            #[cfg(feature = "dblib")]
            Some(Stmt::Dblib(s)) => s.bind_double(idx, val) as i64,
            #[cfg(feature = "firebird")]
            Some(Stmt::Firebird(s)) => s.bind_double(idx, val) as i64,
            #[cfg(any(feature = "odbc", feature = "informix"))]
            Some(Stmt::Odbc(s)) => s.bind_double(idx, val) as i64,
            #[cfg(feature = "oci")]
            Some(Stmt::Oci(s)) => s.bind_double(idx, val) as i64,
            Some(Stmt::Sqlite(s)) => s.bind_double(idx, val),
            Some(Stmt::Postgres(s)) => s.bind(idx, pg::Bind::Float(val)),
            Some(Stmt::Mysql(s)) => s.bind(idx, my::Bind::Float(val)),
            None => 0,
        }
    })
}

/// Binds a text value to the 1-based placeholder `idx`, using the
/// caller-supplied `len` (the value's true byte length) rather than a
/// NUL-terminated-string decode, so a value with an embedded NUL byte binds in
/// full instead of silently truncating at the first NUL (v20/P0-A). A null
/// pointer binds SQL NULL. Returns `1`/`0`; a caught panic reports the `0` failure
/// sentinel.
///
/// # Safety
/// `val`, when non-null, must point to at least `len` readable bytes valid for
/// the call.
#[no_mangle]
pub unsafe extern "C" fn elephc_pdo_bind_text(
    stmt_id: i64,
    idx: i64,
    val: *const c_char,
    len: i64,
) -> i64 {
    ffi_guard(0, || {
        let mut guard = lock_recover(stmts());
        match guard.get_mut(&stmt_id) {
            #[cfg(feature = "dblib")]
            Some(Stmt::Dblib(s)) => {
                if val.is_null() {
                    s.bind_null(idx) as i64
                } else {
                    s.bind_text(idx, bytes_arg(val, len), false) as i64
                }
            }
            #[cfg(feature = "firebird")]
            Some(Stmt::Firebird(s)) => {
                if val.is_null() {
                    s.bind_null(idx) as i64
                } else {
                    s.bind_text(idx, bytes_arg(val, len)) as i64
                }
            }
            #[cfg(any(feature = "odbc", feature = "informix"))]
            Some(Stmt::Odbc(s)) => {
                if val.is_null() {
                    s.bind_null(idx) as i64
                } else {
                    s.bind_text(idx, bytes_arg(val, len)) as i64
                }
            }
            #[cfg(feature = "oci")]
            Some(Stmt::Oci(s)) => {
                if val.is_null() {
                    s.bind_null(idx) as i64
                } else {
                    s.bind_text(idx, bytes_arg(val, len)) as i64
                }
            }
            Some(Stmt::Sqlite(s)) => s.bind_text(idx, val, len),
            Some(Stmt::Postgres(s)) => {
                let bind = if val.is_null() {
                    pg::Bind::Null
                } else {
                    pg::Bind::Text(String::from_utf8_lossy(&bytes_arg(val, len)).into_owned())
                };
                s.bind(idx, bind)
            }
            Some(Stmt::Mysql(s)) => {
                let bind = if val.is_null() {
                    my::Bind::Null
                } else {
                    my::Bind::Text(String::from_utf8_lossy(&bytes_arg(val, len)).into_owned())
                };
                s.bind(idx, bind)
            }
            None => 0,
        }
    })
}

/// Binds a string with MySQL's national-character marker. SQLite and PostgreSQL
/// treat it as ordinary text; MySQL's emulated-prepare renderer emits `N'…'`,
/// while native prepares send the same byte payload as an ordinary string.
///
/// # Safety
/// `val`, when non-null, must point to at least `len` readable bytes valid for
/// the call.
#[no_mangle]
pub unsafe extern "C" fn elephc_pdo_bind_text_national(
    stmt_id: i64,
    idx: i64,
    val: *const c_char,
    len: i64,
) -> i64 {
    ffi_guard(0, || {
        let mut guard = lock_recover(stmts());
        match guard.get_mut(&stmt_id) {
            #[cfg(feature = "dblib")]
            Some(Stmt::Dblib(s)) => {
                if val.is_null() {
                    s.bind_null(idx) as i64
                } else {
                    s.bind_text(idx, bytes_arg(val, len), true) as i64
                }
            }
            #[cfg(feature = "firebird")]
            Some(Stmt::Firebird(s)) => {
                if val.is_null() {
                    s.bind_null(idx) as i64
                } else {
                    s.bind_text(idx, bytes_arg(val, len)) as i64
                }
            }
            #[cfg(any(feature = "odbc", feature = "informix"))]
            Some(Stmt::Odbc(s)) => {
                if val.is_null() {
                    s.bind_null(idx) as i64
                } else {
                    s.bind_text(idx, bytes_arg(val, len)) as i64
                }
            }
            #[cfg(feature = "oci")]
            Some(Stmt::Oci(s)) => {
                if val.is_null() {
                    s.bind_null(idx) as i64
                } else {
                    s.bind_text(idx, bytes_arg(val, len)) as i64
                }
            }
            Some(Stmt::Sqlite(s)) => s.bind_text(idx, val, len),
            Some(Stmt::Postgres(s)) => {
                let bind = if val.is_null() {
                    pg::Bind::Null
                } else {
                    pg::Bind::Text(String::from_utf8_lossy(&bytes_arg(val, len)).into_owned())
                };
                s.bind(idx, bind)
            }
            Some(Stmt::Mysql(s)) => {
                let bind = if val.is_null() {
                    my::Bind::Null
                } else {
                    my::Bind::NationalText(
                        String::from_utf8_lossy(&bytes_arg(val, len)).into_owned(),
                    )
                };
                s.bind(idx, bind)
            }
            None => 0,
        }
    })
}

/// Binds SQL NULL to the 1-based placeholder `idx`. Returns `1`/`0`; a caught panic
/// reports the `0` failure sentinel.
#[no_mangle]
pub extern "C" fn elephc_pdo_bind_null(stmt_id: i64, idx: i64) -> i64 {
    ffi_guard(0, || {
        let mut guard = lock_recover(stmts());
        match guard.get_mut(&stmt_id) {
            #[cfg(feature = "dblib")]
            Some(Stmt::Dblib(s)) => s.bind_null(idx) as i64,
            #[cfg(feature = "firebird")]
            Some(Stmt::Firebird(s)) => s.bind_null(idx) as i64,
            #[cfg(any(feature = "odbc", feature = "informix"))]
            Some(Stmt::Odbc(s)) => s.bind_null(idx) as i64,
            #[cfg(feature = "oci")]
            Some(Stmt::Oci(s)) => s.bind_null(idx) as i64,
            Some(Stmt::Sqlite(s)) => s.bind_null(idx),
            Some(Stmt::Postgres(s)) => s.bind(idx, pg::Bind::Null),
            Some(Stmt::Mysql(s)) => s.bind(idx, my::Bind::Null),
            None => 0,
        }
    })
}

/// Binds a boolean to the 1-based placeholder `idx`: SQLite and MySQL bind it as
/// an integer `0`/`1`; PostgreSQL binds a real boolean value through the text
/// `'t'`/`'f'` parameter format PostgreSQL accepts for `bool` columns (and
/// coerces from for untyped/text columns, matching PDO/PHP's text-parameter
/// convention). Returns `1`/`0`; a caught panic reports the `0` failure sentinel.
#[no_mangle]
pub extern "C" fn elephc_pdo_bind_bool(stmt_id: i64, idx: i64, val: i64) -> i64 {
    ffi_guard(0, || {
        let truthy = (val != 0) as i64;
        let mut guard = lock_recover(stmts());
        match guard.get_mut(&stmt_id) {
            #[cfg(feature = "dblib")]
            Some(Stmt::Dblib(s)) => s.bind_int(idx, truthy) as i64,
            #[cfg(feature = "firebird")]
            Some(Stmt::Firebird(s)) => s.bind_int(idx, truthy) as i64,
            #[cfg(any(feature = "odbc", feature = "informix"))]
            Some(Stmt::Odbc(s)) => s.bind_int(idx, truthy) as i64,
            #[cfg(feature = "oci")]
            Some(Stmt::Oci(s)) => s.bind_int(idx, truthy) as i64,
            Some(Stmt::Sqlite(s)) => s.bind_int(idx, truthy),
            Some(Stmt::Postgres(s)) => {
                let text = if truthy != 0 { "t" } else { "f" };
                s.bind(idx, pg::Bind::Text(text.to_string()))
            }
            Some(Stmt::Mysql(s)) => s.bind(idx, my::Bind::Int(truthy)),
            None => 0,
        }
    })
}

/// Binds raw bytes (embedded NUL preserved) to the 1-based placeholder `idx`:
/// SQLite copies them via `SQLITE_TRANSIENT` (`sqlite3_bind_blob`); PostgreSQL and
/// MySQL bind them through each driver's raw-bytes value path (bypassing the
/// text re-encoding the other bind functions use), so arbitrary binary content
/// round-trips unchanged. Returns `1`/`0`; a caught panic reports the `0` failure
/// sentinel.
///
/// # Safety
/// `ptr`, when non-null, must point to at least `len` readable bytes valid for
/// the call.
#[no_mangle]
pub unsafe extern "C" fn elephc_pdo_bind_blob(
    stmt_id: i64,
    idx: i64,
    ptr: *const c_char,
    len: i64,
) -> i64 {
    ffi_guard(0, || {
        let mut guard = lock_recover(stmts());
        match guard.get_mut(&stmt_id) {
            #[cfg(feature = "dblib")]
            Some(Stmt::Dblib(s)) => {
                if ptr.is_null() {
                    s.bind_null(idx) as i64
                } else {
                    s.bind_blob(idx, bytes_arg(ptr, len)) as i64
                }
            }
            #[cfg(feature = "firebird")]
            Some(Stmt::Firebird(s)) => {
                if ptr.is_null() {
                    s.bind_null(idx) as i64
                } else {
                    s.bind_blob(idx, bytes_arg(ptr, len)) as i64
                }
            }
            #[cfg(any(feature = "odbc", feature = "informix"))]
            Some(Stmt::Odbc(s)) => {
                if ptr.is_null() {
                    s.bind_null(idx) as i64
                } else {
                    s.bind_blob(idx, bytes_arg(ptr, len)) as i64
                }
            }
            #[cfg(feature = "oci")]
            Some(Stmt::Oci(s)) => {
                if ptr.is_null() {
                    s.bind_null(idx) as i64
                } else {
                    s.bind_blob(idx, bytes_arg(ptr, len)) as i64
                }
            }
            Some(Stmt::Sqlite(s)) => s.bind_blob(idx, ptr, len),
            Some(Stmt::Postgres(s)) => {
                let bind = if ptr.is_null() {
                    pg::Bind::Null
                } else {
                    pg::Bind::Bytes(bytes_arg(ptr, len))
                };
                s.bind(idx, bind)
            }
            Some(Stmt::Mysql(s)) => {
                let bind = if ptr.is_null() {
                    my::Bind::Null
                } else {
                    my::Bind::Bytes(bytes_arg(ptr, len))
                };
                s.bind(idx, bind)
            }
            None => 0,
        }
    })
}

/// Marks a one-based native bind as input/output and records its output buffer size.
/// Drivers without output-parameter support accept the common ABI call as a no-op.
#[no_mangle]
pub extern "C" fn elephc_pdo_bind_output(
    stmt_id: i64,
    idx: i64,
    pdo_type: i64,
    max_length: i64,
) -> i64 {
    ffi_guard(0, || {
        let _ = (idx, pdo_type, max_length);
        let mut guard = lock_recover(stmts());
        match guard.get_mut(&stmt_id) {
            #[cfg(any(feature = "odbc", feature = "informix"))]
            Some(Stmt::Odbc(statement)) => {
                statement.bind_output(idx, pdo_type, max_length)
            }
            #[cfg(feature = "oci")]
            Some(Stmt::Oci(statement)) => {
                statement.bind_output(idx, pdo_type, max_length) as i64
            }
            Some(_) => 1,
            None => 0,
        }
    })
}

/// Copies one completed native output bind into the shared binary buffer.
/// Returns its byte length, `-2` for SQL NULL, or `-3` when the slot is not an
/// OCI output bind.
#[no_mangle]
pub extern "C" fn elephc_pdo_output_data(stmt_id: i64, idx: i64) -> i64 {
    ffi_guard(-3, || {
        let _ = (stmt_id, idx);
        let value: Option<Option<Vec<u8>>> = {
            let guard = lock_recover(stmts());
            match guard.get(&stmt_id) {
                #[cfg(any(feature = "odbc", feature = "informix"))]
                Some(Stmt::Odbc(statement)) => {
                    statement.output_value(idx).map(|value| value.data.clone())
                }
                #[cfg(feature = "oci")]
                Some(Stmt::Oci(statement)) => {
                    statement.output_value(idx).map(|value| value.data.clone())
                }
                _ => None,
            }
        };
        let Some(value) = value else {
            return -3;
        };
        let Some(data) = value else {
            return -2;
        };
        let length = data.len() as i64;
        *lock_recover(blob_cell()) = data;
        length
    })
}

/// Reports whether one completed native output bind is a LOB locator.
#[no_mangle]
pub extern "C" fn elephc_pdo_output_is_lob(stmt_id: i64, idx: i64) -> i64 {
    ffi_guard(0, || {
        let _ = idx;
        let guard = lock_recover(stmts());
        match guard.get(&stmt_id) {
            #[cfg(any(feature = "odbc", feature = "informix"))]
            Some(Stmt::Odbc(statement)) => statement
                .output_value(idx)
                .map_or(0, |value| value.lob as i64),
            #[cfg(feature = "oci")]
            Some(Stmt::Oci(statement)) => statement
                .output_value(idx)
                .map_or(0, |value| value.lob as i64),
            _ => 0,
        }
    })
}

/// Resets a statement, keeping its parameter bindings. Returns `1`/`0`; a caught
/// panic reports the `0` failure sentinel.
#[no_mangle]
pub extern "C" fn elephc_pdo_reset(stmt_id: i64) -> i64 {
    ffi_guard(0, || {
        let mut guard = lock_recover(stmts());
        match guard.get_mut(&stmt_id) {
            #[cfg(feature = "dblib")]
            Some(Stmt::Dblib(s)) => {
                s.reset();
                1
            }
            #[cfg(feature = "firebird")]
            Some(Stmt::Firebird(s)) => {
                s.reset();
                1
            }
            #[cfg(any(feature = "odbc", feature = "informix"))]
            Some(Stmt::Odbc(s)) => {
                s.reset();
                1
            }
            #[cfg(feature = "oci")]
            Some(Stmt::Oci(s)) => {
                s.reset();
                1
            }
            Some(Stmt::Sqlite(s)) => s.reset(),
            Some(Stmt::Postgres(s)) => {
                let mut connections = lock_recover(conns());
                match connections.get_mut(&s.conn_id) {
                    Some(Conn::Postgres(connection)) => s.reset(connection),
                    _ => 0,
                }
            }
            Some(Stmt::Mysql(s)) => {
                if let Some(Conn::Mysql(connection)) = lock_recover(conns()).get_mut(&s.conn_id) {
                    return s.reset(connection);
                }
                0
            }
            None => 0,
        }
    })
}

/// Clears all parameter bindings on a statement. Returns `1`/`0`; a caught panic
/// reports the `0` failure sentinel.
#[no_mangle]
pub extern "C" fn elephc_pdo_clear_bindings(stmt_id: i64) -> i64 {
    ffi_guard(0, || {
        let mut guard = lock_recover(stmts());
        match guard.get_mut(&stmt_id) {
            #[cfg(feature = "dblib")]
            Some(Stmt::Dblib(s)) => {
                s.clear_bindings();
                1
            }
            #[cfg(feature = "firebird")]
            Some(Stmt::Firebird(s)) => {
                s.clear_bindings();
                1
            }
            #[cfg(any(feature = "odbc", feature = "informix"))]
            Some(Stmt::Odbc(s)) => {
                s.clear_bindings();
                1
            }
            #[cfg(feature = "oci")]
            Some(Stmt::Oci(s)) => {
                s.clear_bindings();
                1
            }
            Some(Stmt::Sqlite(s)) => s.clear_bindings(),
            Some(Stmt::Postgres(s)) => s.clear_bindings(),
            Some(Stmt::Mysql(s)) => s.clear_bindings(),
            None => 0,
        }
    })
}

/// Advances the statement one row: `1` for a row, `0` when exhausted, `-1` on
/// error — the same `-1` a caught panic degrades to (the client crates are the most
/// likely source of an unexpected panic, and this is where they run).
#[no_mangle]
pub extern "C" fn elephc_pdo_step(stmt_id: i64) -> i64 {
    ffi_guard(-1, || {
        let sqlite_statement = {
            let mut guard = lock_recover(stmts());
            match guard.remove(&stmt_id) {
                Some(Stmt::Sqlite(statement)) => Some(statement),
                Some(other) => {
                    guard.insert(stmt_id, other);
                    None
                }
                None => None,
            }
        };
        if let Some(statement) = sqlite_statement {
            let result = statement.step();
            lock_recover(stmts()).insert(stmt_id, Stmt::Sqlite(statement));
            return result;
        }
        let mut sguard = lock_recover(stmts());
        match sguard.get_mut(&stmt_id) {
            #[cfg(feature = "dblib")]
            Some(Stmt::Dblib(s)) => {
                if s.needs_execute() {
                    let conn_id = s.conn_id;
                    let mut cguard = lock_recover(conns());
                    match cguard.get_mut(&conn_id) {
                        Some(Conn::Dblib(c)) => {
                            if s.execute(c).is_err() {
                                return -1;
                            }
                        }
                        _ => return -1,
                    }
                }
                s.step()
            }
            #[cfg(feature = "firebird")]
            Some(Stmt::Firebird(s)) => {
                if s.needs_execute() {
                    let conn_id = s.conn_id;
                    let mut cguard = lock_recover(conns());
                    match cguard.get_mut(&conn_id) {
                        Some(Conn::Firebird(c)) => {
                            if s.execute(c).is_err() {
                                return -1;
                            }
                        }
                        _ => return -1,
                    }
                }
                s.step()
            }
            #[cfg(any(feature = "odbc", feature = "informix"))]
            Some(Stmt::Odbc(s)) => {
                if s.needs_execute() {
                    let conn_id = s.conn_id;
                    let mut cguard = lock_recover(conns());
                    match cguard.get_mut(&conn_id) {
                        Some(Conn::Odbc(c)) => {
                            if s.execute(c).is_err() {
                                return -1;
                            }
                        }
                        _ => return -1,
                    }
                }
                s.step()
            }
            #[cfg(feature = "oci")]
            Some(Stmt::Oci(s)) => {
                if s.needs_execute() {
                    let conn_id = s.conn_id;
                    let mut cguard = lock_recover(conns());
                    match cguard.get_mut(&conn_id) {
                        Some(Conn::Oci(c)) => {
                            if s.execute(c).is_err() {
                                return -1;
                            }
                        }
                        _ => return -1,
                    }
                }
                s.step()
            }
            Some(Stmt::Postgres(s)) => {
                let conn_id = s.conn_id;
                let mut cguard = lock_recover(conns());
                match cguard.get_mut(&conn_id) {
                    Some(Conn::Postgres(c)) => s.step(c),
                    _ => -1,
                }
            }
            Some(Stmt::Mysql(s)) => {
                let conn_id = s.conn_id;
                let mut cguard = lock_recover(conns());
                match cguard.get_mut(&conn_id) {
                    Some(Conn::Mysql(c)) => s.step(c),
                    _ => -1,
                }
            }
            Some(Stmt::Sqlite(_)) | None => -1,
        }
    })
}

/// Moves a PostgreSQL result according to a PDO fetch orientation and offset.
/// Returns `1` for an active row, `0` when the target is outside the result or
/// for a non-PostgreSQL statement, and `-1` on execution failure or panic.
#[no_mangle]
pub extern "C" fn elephc_pdo_step_oriented(
    stmt_id: i64,
    orientation: i64,
    offset: i64,
) -> i64 {
    ffi_guard(-1, || {
        let mut sguard = lock_recover(stmts());
        match sguard.get_mut(&stmt_id) {
            Some(Stmt::Postgres(s)) => {
                let conn_id = s.conn_id;
                let mut cguard = lock_recover(conns());
                match cguard.get_mut(&conn_id) {
                    Some(Conn::Postgres(c)) => s.step_oriented(c, orientation, offset),
                    _ => -1,
                }
            }
            #[cfg(feature = "dblib")]
            Some(Stmt::Dblib(_)) => 0,
            #[cfg(feature = "firebird")]
            Some(Stmt::Firebird(_)) => 0,
            #[cfg(any(feature = "odbc", feature = "informix"))]
            Some(Stmt::Odbc(s)) => {
                if s.needs_execute() {
                    let conn_id = s.conn_id;
                    let mut cguard = lock_recover(conns());
                    match cguard.get_mut(&conn_id) {
                        Some(Conn::Odbc(c)) => {
                            if s.execute(c).is_err() {
                                return -1;
                            }
                        }
                        _ => return -1,
                    }
                }
                s.step_oriented(orientation, offset)
            }
            #[cfg(feature = "oci")]
            Some(Stmt::Oci(s)) => {
                if s.needs_execute() {
                    let conn_id = s.conn_id;
                    let mut cguard = lock_recover(conns());
                    match cguard.get_mut(&conn_id) {
                        Some(Conn::Oci(c)) => {
                            if s.execute(c).is_err() {
                                return -1;
                            }
                        }
                        _ => return -1,
                    }
                }
                s.step_oriented(orientation, offset)
            }
            Some(Stmt::Sqlite(_)) | Some(Stmt::Mysql(_)) => 0,
            None => -1,
        }
    })
}

/// Returns PDO_DBLIB's native DB-Library type identifier for one result column,
/// or `0` for another driver, an invalid handle/index, or a caught panic.
#[no_mangle]
pub extern "C" fn elephc_pdo_dblib_column_native_type_id(stmt_id: i64, i: i64) -> i64 {
    ffi_guard(0, || {
        #[cfg(feature = "dblib")]
        if let Some(Stmt::Dblib(statement)) = lock_recover(stmts()).get(&stmt_id) {
            return statement.column_native_type_id(i);
        }
        let _ = (stmt_id, i);
        0
    })
}

/// Returns PDO_DBLIB's native server user-type identifier for one result column,
/// or `0` for another driver, invalid input, or a caught panic.
#[no_mangle]
pub extern "C" fn elephc_pdo_dblib_column_user_type_id(stmt_id: i64, i: i64) -> i64 {
    ffi_guard(0, || {
        #[cfg(feature = "dblib")]
        if let Some(Stmt::Dblib(statement)) = lock_recover(stmts()).get(&stmt_id) {
            return statement.column_user_type_id(i);
        }
        let _ = (stmt_id, i);
        0
    })
}

/// Returns PDO_DBLIB's scale for one result column, or `0` for another driver,
/// invalid input, or a caught panic.
#[no_mangle]
pub extern "C" fn elephc_pdo_dblib_column_scale(stmt_id: i64, i: i64) -> i64 {
    ffi_guard(0, || {
        #[cfg(feature = "dblib")]
        if let Some(Stmt::Dblib(statement)) = lock_recover(stmts()).get(&stmt_id) {
            return statement.column_scale(i);
        }
        let _ = (stmt_id, i);
        0
    })
}

/// Returns PDO_DBLIB's source label for one result column. The pointer remains
/// valid until the next call to this accessor; unsupported/invalid input is empty.
#[no_mangle]
pub extern "C" fn elephc_pdo_dblib_column_source(
    stmt_id: i64,
    i: i64,
) -> *const c_char {
    ffi_guard(static_cstr(b"\0"), || {
        let source = {
            #[cfg(feature = "dblib")]
            if let Some(Stmt::Dblib(statement)) = lock_recover(stmts()).get(&stmt_id) {
                return store_cstr(dblib_column_source_cell(), &statement.column_source(i));
            }
            let _ = (stmt_id, i);
            String::new()
        };
        store_cstr(dblib_column_source_cell(), &source)
    })
}

/// Returns the bytes owned by an executed PostgreSQL statement's materialized
/// result. `-1` means unexecuted/unknown/non-PostgreSQL and records php-src's
/// HY000 unexecuted-statement diagnostic on the owning connection.
#[no_mangle]
pub extern "C" fn elephc_pdo_result_memory_size(stmt_id: i64) -> i64 {
    ffi_guard(-1, || {
        let mut sguard = lock_recover(stmts());
        let Some(Stmt::Postgres(statement)) = sguard.get_mut(&stmt_id) else {
            return -1;
        };
        if let Some(bytes) = statement.result_memory_size() {
            return bytes;
        }
        let conn_id = statement.conn_id;
        if let Some(Conn::Postgres(connection)) = lock_recover(conns()).get_mut(&conn_id) {
            connection.sqlstate = "HY000".to_string();
            connection.errcode = 0;
            connection.errmsg = format!(
                "statement '{}' has not been executed yet",
                statement.query_string
            );
        }
        -1
    })
}

/// Advances a MySQL statement to its next materialized protocol result set.
/// Returns `1` when one became active and `0` for no further set, a non-MySQL
/// statement, an unknown handle, or a caught panic.
#[no_mangle]
pub extern "C" fn elephc_pdo_next_rowset(stmt_id: i64) -> i64 {
    ffi_guard(0, || {
        let mut sguard = lock_recover(stmts());
        #[cfg(feature = "dblib")]
        if let Some(Stmt::Dblib(statement)) = sguard.get_mut(&stmt_id) {
            if !statement.next_rowset() {
                return 0;
            }
            let conn_id = statement.conn_id;
            let row_count = statement.current_row_count();
            if let Some(Conn::Dblib(connection)) = lock_recover(conns()).get_mut(&conn_id) {
                connection.changes = row_count;
            }
            return 1;
        }
        #[cfg(feature = "firebird")]
        if matches!(sguard.get(&stmt_id), Some(Stmt::Firebird(_))) {
            return 0;
        }
        #[cfg(any(feature = "odbc", feature = "informix"))]
        if let Some(Stmt::Odbc(statement)) = sguard.get_mut(&stmt_id) {
            let conn_id = statement.conn_id;
            let mut cguard = lock_recover(conns());
            return match cguard.get_mut(&conn_id) {
                Some(Conn::Odbc(connection)) => statement.next_rowset(connection) as i64,
                _ => 0,
            };
        }
        let Some(Stmt::Mysql(statement)) = sguard.get_mut(&stmt_id) else {
            return 0;
        };
        let conn_id = statement.conn_id;
        let mut cguard = lock_recover(conns());
        match cguard.get_mut(&conn_id) {
            Some(Conn::Mysql(connection)) => statement.next_rowset(connection),
            _ => 0,
        }
    })
}

/// Returns the number of result columns for the statement. Unknown handles — and a
/// caught panic — report `0`.
#[no_mangle]
pub extern "C" fn elephc_pdo_column_count(stmt_id: i64) -> i64 {
    ffi_guard(0, || {
        let guard = lock_recover(stmts());
        match guard.get(&stmt_id) {
            #[cfg(feature = "dblib")]
            Some(Stmt::Dblib(s)) => s.column_count(),
            #[cfg(feature = "firebird")]
            Some(Stmt::Firebird(s)) => s.column_count(),
            #[cfg(any(feature = "odbc", feature = "informix"))]
            Some(Stmt::Odbc(s)) => s.column_count(),
            #[cfg(feature = "oci")]
            Some(Stmt::Oci(s)) => s.column_count(),
            Some(Stmt::Sqlite(s)) => s.column_count(),
            Some(Stmt::Postgres(s)) => s.column_count(),
            Some(Stmt::Mysql(s)) => s.column_count(),
            None => 0,
        }
    })
}

/// Returns a pointer to the name of result column `i` (0-based). Unknown handles —
/// and a caught panic — report the empty string.
#[no_mangle]
pub extern "C" fn elephc_pdo_column_name(stmt_id: i64, i: i64) -> *const c_char {
    ffi_guard(static_cstr(b"\0"), || {
        let name = {
            let guard = lock_recover(stmts());
            match guard.get(&stmt_id) {
                #[cfg(feature = "dblib")]
                Some(Stmt::Dblib(s)) => s.column_name(i),
                #[cfg(feature = "firebird")]
                Some(Stmt::Firebird(s)) => s.column_name(i),
                #[cfg(any(feature = "odbc", feature = "informix"))]
                Some(Stmt::Odbc(s)) => s.column_name(i),
                #[cfg(feature = "oci")]
                Some(Stmt::Oci(s)) => s.column_name(i),
                Some(Stmt::Sqlite(s)) => s.column_name(i),
                Some(Stmt::Postgres(s)) => s.column_name(i),
                Some(Stmt::Mysql(s)) => s.column_name(i),
                None => String::new(),
            }
        };
        store_cstr(colname_cell(), &name)
    })
}

/// Returns the SQLite-compatible type code for the current row's column `i`
/// (0-based): 1=int, 2=float, 3=text, 4=blob/bytea, 5=null. Unknown handles — and a
/// caught panic — report `5` ("null"), the type that carries no payload to read.
#[no_mangle]
pub extern "C" fn elephc_pdo_column_type(stmt_id: i64, i: i64) -> i64 {
    ffi_guard(5, || {
        let guard = lock_recover(stmts());
        match guard.get(&stmt_id) {
            #[cfg(feature = "dblib")]
            Some(Stmt::Dblib(s)) => s.column_type(i),
            #[cfg(feature = "firebird")]
            Some(Stmt::Firebird(s)) => s.column_type(i),
            #[cfg(any(feature = "odbc", feature = "informix"))]
            Some(Stmt::Odbc(s)) => s.column_type(i),
            #[cfg(feature = "oci")]
            Some(Stmt::Oci(s)) => s.column_type(i),
            Some(Stmt::Sqlite(s)) => s.column_type(i),
            Some(Stmt::Postgres(s)) => s.column_type(i),
            Some(Stmt::Mysql(s)) => s.column_type(i),
            None => 5,
        }
    })
}

/// Returns a pointer to the declared type of result column `i` (0-based) for a
/// SQLite statement (`sqlite3_column_decltype`), or an empty string for a
/// non-SQLite statement or an expression column. Feeds `getColumnMeta`'s
/// native_type. Valid until the next `elephc_pdo_column_decltype`. A caught panic
/// degrades to that same empty string.
#[no_mangle]
pub extern "C" fn elephc_pdo_column_decltype(stmt_id: i64, i: i64) -> *const c_char {
    ffi_guard(static_cstr(b"\0"), || {
        let decltype = {
            let guard = lock_recover(stmts());
            match guard.get(&stmt_id) {
                Some(Stmt::Sqlite(s)) => s.column_decltype(i),
                _ => String::new(),
            }
        };
        store_cstr(decltype_cell(), &decltype)
    })
}

/// Returns a pointer to the driver-native type name of result column `i`
/// (0-based), as that driver's own catalog spells it, for `getColumnMeta`'s
/// `native_type`:
/// - `pgsql:` — the server's `pg_type.typname` (`int4`, `bool`, `bytea`, …),
///   resolved from the column's `postgres::types::Type` at prepare time (P2-k).
/// - `mysql:` — the wire column type's MySQL name (`LONG`, `VAR_STRING`, `BIT`,
///   `NEWDECIMAL`, …), reproducing php-src's `type_to_name_native` switch
///   (`ext/pdo_mysql/mysql_statement.c:716-770`), whose
///   `PDO_MYSQL_NATIVE_TYPE_NAME(x)` macro stringifies the `MYSQL_TYPE_` suffix —
///   so an `INT` column is `LONG` and a `VARCHAR` is `VAR_STRING`, not the
///   friendlier SQL spelling (F-MY-08).
/// - `sqlite:` — deliberately empty. php-src's SQLite driver reports the column's
///   storage class, which the prelude already derives itself from the live value;
///   its declared type is a separate key, served by `elephc_pdo_column_decltype`.
///
/// Empty for a SQLite statement, an unknown handle, an out-of-range index, or a
/// wire type php-src's own switch has no case for (its `default: return NULL`,
/// which makes php-src OMIT the key entirely — `mysql_statement.c:812-815`). The
/// prelude reads that empty string as "keep the generic storage-class metadata".
/// Valid until the next `elephc_pdo_column_native_type`. A caught panic degrades
/// to that same empty string, i.e. to the generic metadata path.
#[no_mangle]
pub extern "C" fn elephc_pdo_column_native_type(stmt_id: i64, i: i64) -> *const c_char {
    ffi_guard(static_cstr(b"\0"), || {
        let native = {
            let guard = lock_recover(stmts());
            match guard.get(&stmt_id) {
                #[cfg(feature = "dblib")]
                Some(Stmt::Dblib(s)) => s.column_native_type(i),
                #[cfg(feature = "firebird")]
                Some(Stmt::Firebird(s)) => s.column_native_type(i),
                #[cfg(any(feature = "odbc", feature = "informix"))]
                Some(Stmt::Odbc(s)) => s.column_native_type(i),
                #[cfg(feature = "oci")]
                Some(Stmt::Oci(s)) => s.column_native_type(i),
                Some(Stmt::Postgres(s)) => s.column_native_type(i),
                Some(Stmt::Mysql(s)) => s.column_native_type(i),
                _ => String::new(),
            }
        };
        store_cstr(native_type_cell(), &native)
    })
}

/// Returns the native source-table name for result column `i`, or an empty
/// string for expressions, unknown handles, and out-of-range columns. SQLite
/// and MySQL expose it directly in column metadata; PostgreSQL resolves the
/// RowDescription table OID through `pg_catalog.pg_class` at prepare time.
#[no_mangle]
pub extern "C" fn elephc_pdo_column_table_name(stmt_id: i64, i: i64) -> *const c_char {
    ffi_guard(static_cstr(b"\0"), || {
        let table = {
            let guard = lock_recover(stmts());
            match guard.get(&stmt_id) {
                #[cfg(feature = "dblib")]
                Some(Stmt::Dblib(_)) => String::new(),
                #[cfg(feature = "firebird")]
                Some(Stmt::Firebird(_)) => String::new(),
                #[cfg(any(feature = "odbc", feature = "informix"))]
                Some(Stmt::Odbc(s)) => s.column_table_name(i),
                #[cfg(feature = "oci")]
                Some(Stmt::Oci(_)) => String::new(),
                Some(Stmt::Sqlite(statement)) => statement.column_table_name(i),
                Some(Stmt::Postgres(statement)) => statement.column_table_name(i),
                Some(Stmt::Mysql(statement)) => statement.column_table_name(i),
                None => String::new(),
            }
        };
        store_cstr(table_name_cell(), &table)
    })
}

/// Returns driver-specific result-column flag bits for MySQL and Informix.
#[no_mangle]
pub extern "C" fn elephc_pdo_column_flags(stmt_id: i64, i: i64) -> i64 {
    ffi_guard(0, || match lock_recover(stmts()).get(&stmt_id) {
        #[cfg(any(feature = "odbc", feature = "informix"))]
        Some(Stmt::Odbc(statement)) => statement.column_flags(i),
        Some(Stmt::Mysql(statement)) => statement.column_flags(i),
        _ => 0,
    })
}

/// Returns PDO_INFORMIX's `SQLDescribeCol` scale, or zero for another driver.
#[no_mangle]
pub extern "C" fn elephc_pdo_informix_column_scale(stmt_id: i64, column: i64) -> i64 {
    ffi_guard(0, || {
        #[cfg(any(feature = "odbc", feature = "informix"))]
        if let Some(Stmt::Odbc(statement)) = lock_recover(stmts()).get(&stmt_id) {
            return statement.column_scale(column);
        }
        let _ = (stmt_id, column);
        0
    })
}

/// Returns PDO_INFORMIX's metadata `pdo_type`, defaulting to `PDO::PARAM_STR`.
#[no_mangle]
pub extern "C" fn elephc_pdo_informix_column_pdo_type(stmt_id: i64, column: i64) -> i64 {
    ffi_guard(2, || {
        #[cfg(any(feature = "odbc", feature = "informix"))]
        if let Some(Stmt::Odbc(statement)) = lock_recover(stmts()).get(&stmt_id) {
            return statement.column_pdo_type(column);
        }
        let _ = (stmt_id, column);
        2
    })
}

/// Returns the PostgreSQL type OID of result column `i` (0-based) — the
/// `PQftype` value carried by the column's `postgres::types::Type`. `0` (the
/// invalid OID) for a non-PostgreSQL statement or an out-of-range index. The
/// prelude uses a non-zero value both as the "this is a pg column, describe it
/// natively" signal and to derive the PDO param type (BOOL→PARAM_BOOL,
/// int-family→PARAM_INT, BYTEA→PARAM_LOB, else PARAM_STR) plus the `pgsql:oid`
/// metadata key (P2-k). A caught panic degrades to that same `0`.
#[no_mangle]
pub extern "C" fn elephc_pdo_column_type_oid(stmt_id: i64, i: i64) -> i64 {
    ffi_guard(0, || {
        let guard = lock_recover(stmts());
        match guard.get(&stmt_id) {
            Some(Stmt::Postgres(s)) => s.column_type_oid(i),
            _ => 0,
        }
    })
}

/// Returns the OID of the table result column `i` (0-based) was selected FROM on a
/// `pgsql:` statement — `PQftable()`. Backs `getColumnMeta`'s `pgsql:table_oid`
/// key, which php-src's `pgsql_stmt_get_column_meta` emits UNCONDITIONALLY, `0`
/// included (F-PG-01), so the prelude must emit the key even when this returns `0`.
///
/// `0` is `InvalidOid`, and it is the server's OWN answer for a column that is not
/// a plain table column (an expression, a literal, an aggregate, a function
/// result) — so it doubles as the neutral value here for a non-PostgreSQL
/// statement, an unknown handle, an out-of-range index, and a caught panic. That
/// conflation is intentional and safe: a caller cannot distinguish "not a table
/// column" from "not a pg statement", but neither can real PDO, which only emits
/// the key for a pg statement in the first place.
#[no_mangle]
pub extern "C" fn elephc_pdo_column_table_oid(stmt_id: i64, i: i64) -> i64 {
    ffi_guard(0, || {
        let guard = lock_recover(stmts());
        match guard.get(&stmt_id) {
            Some(Stmt::Postgres(s)) => s.column_table_oid(i),
            _ => 0,
        }
    })
}

/// Returns the byte width of result column `i`'s type (0-based) on a `pgsql:`
/// statement: a positive fixed width (`int4` → 4, `timestamp` → 8, `uuid` → 16),
/// or `-1` for a variable-length (varlena) type — `text`, `varchar`, `numeric`,
/// `bytea`, `json`, any array. Backs `getColumnMeta`'s `len`, which php-src fills
/// from `PQfsize()` (`ext/pdo_pgsql/pgsql_statement.c:496`) (F-PG-02).
///
/// Note that a varlena's `len` is `-1`, NOT its declared `n`: `VARCHAR(20)` reports
/// `-1` here and surfaces its 20 through `elephc_pdo_column_precision` instead (as
/// 24, the raw `atttypmod`). That is real PDO's behavior, not an approximation.
///
/// The value is DERIVED from the column's type rather than read off the wire —
/// tokio-postgres parses the RowDescription's data-type-size field but drops it
/// when building `Column`. This is sound because `PQfsize()` returns
/// `pg_type.typlen`, a property of the TYPE and not of the column or row; see
/// [`pg::PgStmt::column_len`] for the derivation table and its documented edges.
///
/// `-1` — PostgreSQL's own "not a fixed width" — for a non-PostgreSQL statement, an
/// unknown handle, an out-of-range index, and a caught panic.
#[no_mangle]
pub extern "C" fn elephc_pdo_column_len(stmt_id: i64, i: i64) -> i64 {
    ffi_guard(-1, || {
        let guard = lock_recover(stmts());
        match guard.get(&stmt_id) {
            #[cfg(feature = "dblib")]
            Some(Stmt::Dblib(s)) => s.column_len(i),
            #[cfg(feature = "firebird")]
            Some(Stmt::Firebird(_)) => -1,
            Some(Stmt::Postgres(s)) => s.column_len(i),
            Some(Stmt::Mysql(s)) => s.column_len(i),
            _ => -1,
        }
    })
}

/// Returns the type modifier (`atttypmod`) of result column `i` (0-based) on a
/// `pgsql:` statement — `PQfmod()`. Backs `getColumnMeta`'s `precision`, which
/// php-src fills from it straight (`ext/pdo_pgsql/pgsql_statement.c:497`) (F-PG-02).
///
/// The value is the RAW modifier, deliberately NOT decoded, because php-src does
/// not decode it either: `VARCHAR(20)` reports 24 (the length plus `VARHDRSZ` = 4)
/// and `NUMERIC(10,2)` reports 655366 (`((10 << 16) | 2) + 4`). Decoding it into a
/// human-readable precision here would be a divergence from PHP dressed up as an
/// improvement — a caller who wants the real precision must decode the modifier
/// exactly as it would have to against real PDO.
///
/// `-1` — PostgreSQL's own "no type modifier" — for a type that takes no modifier
/// or a column carrying none, and equally for a non-PostgreSQL statement, an
/// unknown handle, an out-of-range index, and a caught panic.
#[no_mangle]
pub extern "C" fn elephc_pdo_column_precision(stmt_id: i64, i: i64) -> i64 {
    ffi_guard(-1, || {
        let guard = lock_recover(stmts());
        match guard.get(&stmt_id) {
            #[cfg(feature = "dblib")]
            Some(Stmt::Dblib(s)) => s.column_precision(i),
            #[cfg(feature = "firebird")]
            Some(Stmt::Firebird(_)) => -1,
            Some(Stmt::Postgres(s)) => s.column_precision(i),
            Some(Stmt::Mysql(s)) => s.column_precision(i),
            Some(Stmt::Sqlite(_)) => 0,
            _ => -1,
        }
    })
}

/// Loads a SQLite extension by path for a `sqlite:` connection
/// (`Pdo\Sqlite::loadExtension()`), returning 1 on success or 0 for a
/// non-SQLite connection, unknown handle, load error, or a caught panic.
///
/// # Safety
/// `path` must point to a NUL-terminated string valid for the call, and loading an
/// extension runs arbitrary native code from it.
#[no_mangle]
pub unsafe extern "C" fn elephc_pdo_load_extension(conn_id: i64, path: *const c_char) -> i64 {
    ffi_guard(0, || {
        let Some(path) = cstr_arg(path) else {
            return 0;
        };
        let guard = lock_recover(conns());
        match guard.get(&conn_id) {
            Some(Conn::Sqlite(c)) => c.load_extension(path),
            _ => 0,
        }
    })
}

/// Registers a custom SQLite collation from a compiled-PHP comparator
/// (`Pdo\Sqlite::createCollation`). `descriptor` is the callable descriptor
/// pointer and `adapter` the codegen collation-adapter address, both produced by
/// the prelude via `__elephc_callable_ptr` / `__elephc_pdo_adapter_addr`. Returns
/// `1` on success, `0` on error, a non-SQLite handle, or a caught panic. Registration
/// itself never fires the comparator (SQLite invokes it later, during an
/// `ORDER BY … COLLATE`), so the connection lock is held only for the brief
/// `sqlite3_create_collation_v2`.
///
/// # Safety
/// `name` must be a NUL-terminated string valid for the call; `descriptor`/`adapter`
/// must be the live callable descriptor and adapter entry of the compiled program.
#[no_mangle]
pub unsafe extern "C" fn elephc_pdo_create_collation(
    conn_id: i64,
    name: *const c_char,
    descriptor: *mut c_void,
    adapter: *mut c_void,
) -> i64 {
    ffi_guard(0, || {
        let Some(name) = cstr_arg(name) else {
            return 0;
        };
        let guard = lock_recover(conns());
        match guard.get(&conn_id) {
            Some(Conn::Sqlite(c)) => c.create_collation(name, descriptor, adapter as *const c_void),
            _ => 0,
        }
    })
}

/// Registers a scalar SQL function `name` backed by a compiled-PHP callable
/// (`Pdo\Sqlite::createFunction`). `num_args` is the declared arity (-1 = variadic),
/// `flags` an optional `SQLITE_DETERMINISTIC`, and `descriptor`/`adapter` the callable
/// descriptor pointer and the codegen scalar adapter address, produced by the prelude
/// via `__elephc_callable_ptr` / `__elephc_pdo_adapter_addr`. Returns `1` on success,
/// `0` on error, a non-SQLite handle, or a caught panic.
///
/// # Safety
/// `name` must be a NUL-terminated string valid for the call; `descriptor`/`adapter`
/// must be the live callable descriptor and adapter entry of the compiled program.
#[no_mangle]
pub unsafe extern "C" fn elephc_pdo_create_function(
    conn_id: i64,
    name: *const c_char,
    num_args: i64,
    flags: i64,
    descriptor: *mut c_void,
    adapter: *mut c_void,
) -> i64 {
    ffi_guard(0, || {
        let Some(name) = cstr_arg(name) else {
            return 0;
        };
        let guard = lock_recover(conns());
        match guard.get(&conn_id) {
            Some(Conn::Sqlite(c)) => {
                c.create_function(name, num_args, flags, descriptor, adapter as *const c_void)
            }
            _ => 0,
        }
    })
}

/// Registers an aggregate SQL function `name` backed by a compiled-PHP step +
/// finalize pair (`Pdo\Sqlite::createAggregate`). `num_args` is the declared arity
/// (-1 = variadic); each callable crosses as a (descriptor, adapter) pointer pair,
/// produced by the prelude via `__elephc_callable_ptr` / `__elephc_pdo_adapter_addr`
/// (kinds 2 and 3). Returns `1` on success, `0` on error, a non-SQLite handle, or a
/// caught panic.
///
/// # Safety
/// `name` must be a NUL-terminated string valid for the call; the four pointers must
/// be the live callable descriptors and adapter entries of the compiled program.
#[no_mangle]
pub unsafe extern "C" fn elephc_pdo_create_aggregate(
    conn_id: i64,
    name: *const c_char,
    num_args: i64,
    step_descriptor: *mut c_void,
    step_adapter: *mut c_void,
    final_descriptor: *mut c_void,
    final_adapter: *mut c_void,
) -> i64 {
    ffi_guard(0, || {
        let Some(name) = cstr_arg(name) else {
            return 0;
        };
        let guard = lock_recover(conns());
        match guard.get(&conn_id) {
            Some(Conn::Sqlite(c)) => c.create_aggregate(
                name,
                num_args,
                step_descriptor,
                step_adapter as *const c_void,
                final_descriptor,
                final_adapter as *const c_void,
            ),
            _ => 0,
        }
    })
}

/// Installs a PHP 8.5 SQLite authorizer callback. The callable descriptor and
/// shared scalar-adapter address are produced by the PDO prelude. Returns `1` on
/// success, `0` for a non-SQLite/unknown connection or a caught panic.
///
/// # Safety
/// `descriptor` and `adapter` must be live compiled-program pointers rooted by
/// the owning PDO object until reset or connection close.
#[no_mangle]
pub unsafe extern "C" fn elephc_pdo_set_authorizer(
    conn_id: i64,
    descriptor: *mut c_void,
    adapter: *mut c_void,
) -> i64 {
    ffi_guard(0, || match lock_recover(conns()).get(&conn_id) {
        Some(Conn::Sqlite(c)) => c.set_authorizer(descriptor, adapter as *const c_void),
        _ => 0,
    })
}

/// Clears a PHP 8.5 SQLite authorizer registration. Returns `1` for a live
/// SQLite connection (including an already-cleared one), otherwise `0`.
#[no_mangle]
pub extern "C" fn elephc_pdo_clear_authorizer(conn_id: i64) -> i64 {
    ffi_guard(0, || match lock_recover(conns()).get(&conn_id) {
        Some(Conn::Sqlite(c)) => {
            c.clear_authorizer();
            1
        }
        _ => 0,
    })
}

/// Clears every SQLite collation, scalar, aggregate, and authorizer callback tied
/// to a connection. Returns `1` for a live SQLite connection and `0` otherwise.
#[no_mangle]
pub extern "C" fn elephc_pdo_clear_callbacks(conn_id: i64) -> i64 {
    ffi_guard(0, || {
        let sqlite_db = match lock_recover(conns()).get(&conn_id) {
            Some(Conn::Sqlite(c)) => c.db,
            _ => return 0,
        };
        // SQLite can refuse to delete a function while a prepared statement still
        // references it. PDO teardown already invalidates statements belonging to
        // the destroyed handle, so finalize those registrations before callbacks.
        let owned: Vec<i64> = lock_recover(stmts())
            .iter()
            .filter_map(|(id, stmt)| match stmt {
                Stmt::Sqlite(stmt) if stmt.db == sqlite_db => Some(*id),
                _ => None,
            })
            .collect();
        {
            let mut statements = lock_recover(stmts());
            for id in owned {
                if let Some(Stmt::Sqlite(stmt)) = statements.get(&id) {
                    stmt.finalize();
                }
                statements.remove(&id);
            }
        }
        match lock_recover(conns()).get(&conn_id) {
            Some(Conn::Sqlite(c)) => {
                c.clear_callbacks();
                1
            }
            _ => 0,
        }
    })
}

/// Takes and clears the deferred PHP error classification from the most recent
/// SQLite authorizer callback. Zero means no callback contract error.
#[no_mangle]
pub extern "C" fn elephc_pdo_take_authorizer_error(conn_id: i64) -> i64 {
    ffi_guard(0, || match lock_recover(conns()).get(&conn_id) {
        Some(Conn::Sqlite(c)) => c.take_authorizer_error(),
        _ => 0,
    })
}

/// Returns the current row's column `i` (0-based) as an integer. Unknown handles —
/// and a caught panic — report `0`.
#[no_mangle]
pub extern "C" fn elephc_pdo_column_int(stmt_id: i64, i: i64) -> i64 {
    ffi_guard(0, || {
        let guard = lock_recover(stmts());
        match guard.get(&stmt_id) {
            #[cfg(feature = "dblib")]
            Some(Stmt::Dblib(s)) => s.column_int(i),
            #[cfg(feature = "firebird")]
            Some(Stmt::Firebird(s)) => s.column_int(i),
            #[cfg(any(feature = "odbc", feature = "informix"))]
            Some(Stmt::Odbc(s)) => s.column_int(i),
            #[cfg(feature = "oci")]
            Some(Stmt::Oci(s)) => s.column_int(i),
            Some(Stmt::Sqlite(s)) => s.column_int(i),
            Some(Stmt::Postgres(s)) => s.column_int(i),
            Some(Stmt::Mysql(s)) => s.column_int(i),
            None => 0,
        }
    })
}

/// Returns the current row's column `i` (0-based) as a double. Unknown handles — and
/// a caught panic — report `0.0`.
#[no_mangle]
pub extern "C" fn elephc_pdo_column_double(stmt_id: i64, i: i64) -> f64 {
    ffi_guard(0.0, || {
        let guard = lock_recover(stmts());
        match guard.get(&stmt_id) {
            #[cfg(feature = "dblib")]
            Some(Stmt::Dblib(s)) => s.column_double(i),
            #[cfg(feature = "firebird")]
            Some(Stmt::Firebird(s)) => s.column_double(i),
            #[cfg(any(feature = "odbc", feature = "informix"))]
            Some(Stmt::Odbc(s)) => s.column_double(i),
            #[cfg(feature = "oci")]
            Some(Stmt::Oci(s)) => s.column_double(i),
            Some(Stmt::Sqlite(s)) => s.column_double(i),
            Some(Stmt::Postgres(s)) => s.column_double(i),
            Some(Stmt::Mysql(s)) => s.column_double(i),
            None => 0.0,
        }
    })
}

/// Returns the byte length of the current row's column `i` rendered as PDO text
/// or BLOB bytes. Paired with `elephc_pdo_column_data_ptr`, this is the only column
/// read path: it is byte-exact, so embedded NUL bytes survive (v24/F-QUAL-03 deleted
/// the NUL-stripping `elephc_pdo_column_text` that used to sit beside it). Unknown
/// handles — and a caught panic — report `0`.
#[no_mangle]
pub extern "C" fn elephc_pdo_column_data_len(stmt_id: i64, i: i64) -> i64 {
    ffi_guard(0, || {
        let guard = lock_recover(stmts());
        match guard.get(&stmt_id) {
            #[cfg(feature = "dblib")]
            Some(Stmt::Dblib(s)) => s.column_data(i).len() as i64,
            #[cfg(feature = "firebird")]
            Some(Stmt::Firebird(s)) => s.column_data(i).len() as i64,
            #[cfg(any(feature = "odbc", feature = "informix"))]
            Some(Stmt::Odbc(s)) => s.column_data(i).len() as i64,
            #[cfg(feature = "oci")]
            Some(Stmt::Oci(s)) => s.column_data(i).len() as i64,
            Some(Stmt::Sqlite(s)) => s.column_data(i).len() as i64,
            Some(Stmt::Postgres(s)) => s.column_data(i).len() as i64,
            Some(Stmt::Mysql(s)) => s.column_data(i).len() as i64,
            None => 0,
        }
    })
}

/// Returns a pointer to the current row's column `i` rendered as raw bytes.
/// The pointer remains valid until the next `elephc_pdo_column_data_ptr` call. An
/// empty column — and a caught panic — report a NULL pointer, which the prelude reads
/// as the empty string (it pairs every read with `elephc_pdo_column_data_len`).
#[no_mangle]
pub extern "C" fn elephc_pdo_column_data_ptr(stmt_id: i64, i: i64) -> *const c_char {
    ffi_guard(std::ptr::null(), || {
        let bytes = {
            let guard = lock_recover(stmts());
            match guard.get(&stmt_id) {
                #[cfg(feature = "dblib")]
                Some(Stmt::Dblib(s)) => s.column_data(i),
                #[cfg(feature = "firebird")]
                Some(Stmt::Firebird(s)) => s.column_data(i),
                #[cfg(any(feature = "odbc", feature = "informix"))]
                Some(Stmt::Odbc(s)) => s.column_data(i),
                #[cfg(feature = "oci")]
                Some(Stmt::Oci(s)) => s.column_data(i),
                Some(Stmt::Sqlite(s)) => s.column_data(i),
                Some(Stmt::Postgres(s)) => s.column_data(i),
                Some(Stmt::Mysql(s)) => s.column_data(i),
                None => Vec::new(),
            }
        };
        store_bytes(bytes)
    })
}

/// Returns one byte from the current row's column `i` rendered as raw data.
/// Out-of-range handles, columns, and offsets return `0` — and so does a caught panic.
#[no_mangle]
pub extern "C" fn elephc_pdo_column_data_byte(stmt_id: i64, i: i64, offset: i64) -> i64 {
    ffi_guard(0, || {
        let Ok(offset) = usize::try_from(offset) else {
            return 0;
        };
        let bytes = {
            let guard = lock_recover(stmts());
            match guard.get(&stmt_id) {
                #[cfg(feature = "dblib")]
                Some(Stmt::Dblib(s)) => s.column_data(i),
                #[cfg(feature = "firebird")]
                Some(Stmt::Firebird(s)) => s.column_data(i),
                #[cfg(any(feature = "odbc", feature = "informix"))]
                Some(Stmt::Odbc(s)) => s.column_data(i),
                #[cfg(feature = "oci")]
                Some(Stmt::Oci(s)) => s.column_data(i),
                Some(Stmt::Sqlite(s)) => s.column_data(i),
                Some(Stmt::Postgres(s)) => s.column_data(i),
                Some(Stmt::Mysql(s)) => s.column_data(i),
                None => Vec::new(),
            }
        };
        bytes.get(offset).copied().unwrap_or(0) as i64
    })
}

/// Finalizes a statement and removes it from the table. Unknown handles return
/// `0`; success returns `1`. A caught panic reports `0` — the statement may then stay
/// registered, which leaks it but keeps the handle table consistent.
#[no_mangle]
pub extern "C" fn elephc_pdo_finalize(stmt_id: i64) -> i64 {
    ffi_guard(0, || {
        match lock_recover(stmts()).remove(&stmt_id) {
            #[cfg(feature = "dblib")]
            Some(Stmt::Dblib(_)) => 1,
            #[cfg(feature = "firebird")]
            Some(Stmt::Firebird(_)) => 1,
            #[cfg(any(feature = "odbc", feature = "informix"))]
            Some(Stmt::Odbc(_)) => 1,
            #[cfg(feature = "oci")]
            Some(Stmt::Oci(_)) => 1,
            Some(Stmt::Sqlite(s)) => {
                s.finalize();
                1
            }
            Some(Stmt::Postgres(mut statement)) => {
                if let Some(Conn::Postgres(connection)) =
                    lock_recover(conns()).get_mut(&statement.conn_id)
                {
                    statement.reset(connection);
                }
                1
            }
            Some(Stmt::Mysql(mut statement)) => {
                if let Some(Conn::Mysql(connection)) =
                    lock_recover(conns()).get_mut(&statement.conn_id)
                {
                    statement.reset(connection);
                }
                1
            }
            None => 0,
        }
    })
}

/// Returns `1` if a SQLite statement makes no direct changes to the database file
/// content (`sqlite3_stmt_readonly`), else `0` — including for a non-SQLite or
/// unknown handle, where the notion does not apply. Backs
/// `PDOStatement::getAttribute(Pdo\Sqlite::ATTR_READONLY_STATEMENT)` (P2-16) as a
/// live read rather than a value stored at prepare time. A caught panic degrades to
/// that same `0`.
#[no_mangle]
pub extern "C" fn elephc_pdo_stmt_readonly(stmt_id: i64) -> i64 {
    ffi_guard(0, || {
        match lock_recover(stmts()).get(&stmt_id) {
            Some(Stmt::Sqlite(s)) => s.readonly(),
            _ => 0,
        }
    })
}

/// Returns SQLite's live busy flag for a statement, or `0` for another driver/handle.
#[no_mangle]
pub extern "C" fn elephc_pdo_stmt_busy(stmt_id: i64) -> i64 {
    ffi_guard(0, || match lock_recover(stmts()).get(&stmt_id) {
        Some(Stmt::Sqlite(statement)) => statement.busy(),
        _ => 0,
    })
}

/// Returns SQLite's explain mode for a statement, or `-1` for another driver/handle.
#[no_mangle]
pub extern "C" fn elephc_pdo_stmt_explain_mode(stmt_id: i64) -> i64 {
    ffi_guard(-1, || match lock_recover(stmts()).get(&stmt_id) {
        Some(Stmt::Sqlite(statement)) => statement.explain_mode(),
        _ => -1,
    })
}

/// Sets SQLite's explain mode for a statement, returning `1` only on success.
#[no_mangle]
pub extern "C" fn elephc_pdo_stmt_set_explain_mode(stmt_id: i64, mode: i64) -> i64 {
    ffi_guard(0, || match lock_recover(stmts()).get(&stmt_id) {
        Some(Stmt::Sqlite(statement)) => statement.set_explain_mode(mode),
        _ => 0,
    })
}

/// Returns the native driver code for the statement's last operation. SQLite
/// tracks this per-connection (mirrored here from the statement's own `db`
/// pointer); PostgreSQL/MySQL statements share their connection's bookkeeping
/// (looked up by the statement's `conn_id`, the same way `elephc_pdo_step`
/// dispatches into the connection to execute). Unknown handles — and a caught panic
/// — return `-1`.
#[no_mangle]
pub extern "C" fn elephc_pdo_stmt_errcode(stmt_id: i64) -> i64 {
    ffi_guard(-1, || {
        let sguard = lock_recover(stmts());
        match sguard.get(&stmt_id) {
            #[cfg(feature = "dblib")]
            Some(Stmt::Dblib(s)) => s.errcode(),
            #[cfg(feature = "firebird")]
            Some(Stmt::Firebird(s)) => s.errcode(),
            #[cfg(any(feature = "odbc", feature = "informix"))]
            Some(Stmt::Odbc(s)) => s.errcode(),
            #[cfg(feature = "oci")]
            Some(Stmt::Oci(s)) => s.errcode(),
            Some(Stmt::Sqlite(s)) => s.errcode(),
            Some(Stmt::Postgres(s)) => {
                let conn_id = s.conn_id;
                let cguard = lock_recover(conns());
                match cguard.get(&conn_id) {
                    Some(Conn::Postgres(c)) => c.errcode,
                    _ => -1,
                }
            }
            Some(Stmt::Mysql(s)) => {
                let conn_id = s.conn_id;
                let cguard = lock_recover(conns());
                match cguard.get(&conn_id) {
                    Some(Conn::Mysql(c)) => c.errcode,
                    _ => -1,
                }
            }
            None => -1,
        }
    })
}

/// Returns a pointer to the statement's last error message (see
/// `elephc_pdo_stmt_errcode` for how PostgreSQL/MySQL statements share their
/// connection's bookkeeping). Empty string for an unknown handle — and for a caught
/// panic. Valid until the next `elephc_pdo_stmt_errmsg`.
#[no_mangle]
pub extern "C" fn elephc_pdo_stmt_errmsg(stmt_id: i64) -> *const c_char {
    ffi_guard(static_cstr(b"\0"), || {
        let msg = {
            let sguard = lock_recover(stmts());
            match sguard.get(&stmt_id) {
                #[cfg(feature = "dblib")]
                Some(Stmt::Dblib(s)) => s.errmsg().to_string(),
                #[cfg(feature = "firebird")]
                Some(Stmt::Firebird(s)) => s.errmsg().to_string(),
                #[cfg(any(feature = "odbc", feature = "informix"))]
                Some(Stmt::Odbc(s)) => s.errmsg().to_string(),
                #[cfg(feature = "oci")]
                Some(Stmt::Oci(s)) => s.errmsg().to_string(),
                Some(Stmt::Sqlite(s)) => s.errmsg(),
                Some(Stmt::Postgres(s)) => {
                    let conn_id = s.conn_id;
                    let cguard = lock_recover(conns());
                    match cguard.get(&conn_id) {
                        Some(Conn::Postgres(c)) => c.errmsg.clone(),
                        _ => String::new(),
                    }
                }
                Some(Stmt::Mysql(s)) => {
                    let conn_id = s.conn_id;
                    let cguard = lock_recover(conns());
                    match cguard.get(&conn_id) {
                        Some(Conn::Mysql(c)) => c.errmsg.clone(),
                        _ => String::new(),
                    }
                }
                None => String::new(),
            }
        };
        store_cstr(stmt_errmsg_cell(), &msg)
    })
}

/// Returns PDO_DBLIB's operating-system error code for statement errorInfo.
#[no_mangle]
pub extern "C" fn elephc_pdo_dblib_stmt_os_errcode(stmt_id: i64) -> i64 {
    ffi_guard(0, || {
        #[cfg(feature = "dblib")]
        if let Some(Stmt::Dblib(statement)) = lock_recover(stmts()).get(&stmt_id) {
            return statement.os_errcode();
        }
        let _ = stmt_id;
        0
    })
}

/// Returns PDO_DBLIB's error severity for statement errorInfo.
#[no_mangle]
pub extern "C" fn elephc_pdo_dblib_stmt_severity(stmt_id: i64) -> i64 {
    ffi_guard(0, || {
        #[cfg(feature = "dblib")]
        if let Some(Stmt::Dblib(statement)) = lock_recover(stmts()).get(&stmt_id) {
            return statement.severity();
        }
        let _ = stmt_id;
        0
    })
}

/// Returns PDO_DBLIB's operating-system statement diagnostic. The pointer remains
/// valid until the next call; unsupported or unknown handles return empty.
#[no_mangle]
pub extern "C" fn elephc_pdo_dblib_stmt_os_errmsg(stmt_id: i64) -> *const c_char {
    ffi_guard(static_cstr(b"\0"), || {
        let message = {
            let guard = lock_recover(stmts());
            match guard.get(&stmt_id) {
                #[cfg(feature = "dblib")]
                Some(Stmt::Dblib(statement)) => statement.os_errmsg().to_string(),
                _ => String::new(),
            }
        };
        store_cstr(dblib_stmt_os_errmsg_cell(), &message)
    })
}

/// Returns the most recently rendered SQL for an emulated MySQL/PostgreSQL
/// statement. Native and SQLite statements, unknown handles and caught panics return
/// an empty string. Valid until the next `elephc_pdo_stmt_sent_sql` call.
#[no_mangle]
pub extern "C" fn elephc_pdo_stmt_sent_sql(stmt_id: i64) -> *const c_char {
    ffi_guard(static_cstr(b"\0"), || {
        let sql = {
            let sguard = lock_recover(stmts());
            match sguard.get(&stmt_id) {
                #[cfg(feature = "dblib")]
                Some(Stmt::Dblib(s)) => s.sent_sql.clone(),
                #[cfg(feature = "firebird")]
                Some(Stmt::Firebird(s)) => s.sent_sql.clone(),
                #[cfg(any(feature = "odbc", feature = "informix"))]
                Some(Stmt::Odbc(s)) => s.sent_sql.clone(),
                #[cfg(feature = "oci")]
                Some(Stmt::Oci(s)) => s.sent_sql.clone(),
                Some(Stmt::Postgres(s)) => s.sent_sql.clone(),
                Some(Stmt::Mysql(s)) => s.sent_sql.clone(),
                Some(Stmt::Sqlite(_)) | None => String::new(),
            }
        };
        store_cstr(stmt_sent_sql_cell(), &sql)
    })
}

/// Returns a pointer to the 5-char SQLSTATE for the statement's last operation
/// (see `elephc_pdo_stmt_errcode` for how PostgreSQL/MySQL statements share their
/// connection's bookkeeping). Unknown handles report `"00000"`; a statement whose
/// connection has since closed reports `"HY000"`. A caught panic degrades to the
/// unknown-handle answer, `"00000"` — the call that panicked has already reported its
/// own failure sentinel, which is what the prelude raises on. Valid until the next
/// `elephc_pdo_stmt_sqlstate`.
#[no_mangle]
pub extern "C" fn elephc_pdo_stmt_sqlstate(stmt_id: i64) -> *const c_char {
    ffi_guard(static_cstr(b"00000\0"), || {
        let state = {
            let sguard = lock_recover(stmts());
            match sguard.get(&stmt_id) {
                #[cfg(feature = "dblib")]
                Some(Stmt::Dblib(s)) => s.sqlstate().to_string(),
                #[cfg(feature = "firebird")]
                Some(Stmt::Firebird(s)) => s.sqlstate().to_string(),
                #[cfg(any(feature = "odbc", feature = "informix"))]
                Some(Stmt::Odbc(s)) => s.sqlstate().to_string(),
                #[cfg(feature = "oci")]
                Some(Stmt::Oci(s)) => s.sqlstate().to_string(),
                Some(Stmt::Sqlite(s)) => s.sqlstate(),
                Some(Stmt::Postgres(s)) => {
                    let conn_id = s.conn_id;
                    let cguard = lock_recover(conns());
                    match cguard.get(&conn_id) {
                        Some(Conn::Postgres(c)) => c.sqlstate.clone(),
                        _ => "HY000".to_string(),
                    }
                }
                Some(Stmt::Mysql(s)) => {
                    let conn_id = s.conn_id;
                    let cguard = lock_recover(conns());
                    match cguard.get(&conn_id) {
                        Some(Conn::Mysql(c)) => c.sqlstate.clone(),
                        _ => "HY000".to_string(),
                    }
                }
                None => "00000".to_string(),
            }
        };
        store_cstr(stmt_sqlstate_cell(), &state)
    })
}

#[cfg(test)]
mod tests {
    //! Unit tests for the PDO bridge, plus the two `#[ignore]` live round-trips
    //! (`pg_round_trip`, `my_round_trip`) that need a real server.
    //!
    //! Live-server environment variables (F-QUAL-06)
    //! ---------------------------------------------
    //! The live tests are split across two suites that historically read DISJOINT
    //! variable families, so exporting one family silently left the other suite's
    //! `#[ignore]` tests doing nothing:
    //!
    //! - `ELEPHC_PG_DSN`      — PostgreSQL DSN. Read by the codegen suite
    //!   (`tests/codegen/pdo_pgsql.rs`) and, as a FALLBACK, by `pg_round_trip` here.
    //! - `ELEPHC_MY_DSN`      — MySQL/MariaDB DSN. Read by the codegen suite
    //!   (`tests/codegen/pdo_mysql.rs`) and, as a FALLBACK, by `my_round_trip` here.
    //! - `ELEPHC_PG_TEST_DSN` — legacy in-crate-only name for the PostgreSQL DSN.
    //!   Still honored, and still takes precedence over `ELEPHC_PG_DSN`.
    //! - `ELEPHC_MY_TEST_DSN` — legacy in-crate-only name for the MySQL DSN.
    //!   Still honored, and still takes precedence over `ELEPHC_MY_DSN`.
    //! - `ELEPHC_PG_TLS_DSN`  — codegen-only: a `sslmode=require` PostgreSQL DSN for
    //!   `pgsql_tls_round_trip`. Needs a TLS-serving server.
    //! - `ELEPHC_MY_TLS_DSN` / `ELEPHC_MY_TLS_CA` — codegen-only: DSN + CA-bundle path
    //!   for `mysql_tls_round_trip`. Needs a TLS-serving server; the default build
    //!   already includes the ring-backed `mysql-tls` feature.
    //!
    //! Because of the fallback, exporting just `ELEPHC_PG_DSN` + `ELEPHC_MY_DSN` now
    //! drives BOTH suites:
    //!
    //! ```text
    //! ELEPHC_PG_DSN='pgsql:host=localhost;port=5432;dbname=testdb;user=test;password=test' \
    //! ELEPHC_MY_DSN='mysql:host=127.0.0.1;port=3306;dbname=testdb;user=test;password=test' \
    //!     cargo test -p elephc-pdo -- --ignored
    //! ```
    //!
    //! - `ELEPHC_PDO_LIVE_REQUIRED` — set to `1` (as the nightly `pdo-live.yml` workflow
    //!   does) to turn "no DSN in the environment" from a SILENT early return into a
    //!   panic. Without it, a renamed or unexported variable makes a live test that ran
    //!   nothing look exactly like a live test that passed — the failure mode that let
    //!   the `CALL` row-drop regression through.

    use super::*;

    /// Resolves a live-server DSN from the first environment variable that is set,
    /// preferring the legacy in-crate name and falling back to the name the codegen
    /// live suite uses (F-QUAL-06), so one set of variables runs every live test.
    ///
    /// Returns `None` — and the caller skips — when neither name is set, unless
    /// `ELEPHC_PDO_LIVE_REQUIRED=1`, in which case the missing DSN is a hard panic
    /// so a misconfigured CI job cannot report a green live run that executed nothing.
    fn live_dsn(primary: &str, fallback: &str) -> Option<String> {
        let dsn = std::env::var(primary)
            .ok()
            .or_else(|| std::env::var(fallback).ok())
            .filter(|dsn| !dsn.trim().is_empty());
        if dsn.is_none() && std::env::var("ELEPHC_PDO_LIVE_REQUIRED").as_deref() == Ok("1") {
            panic!(
                "ELEPHC_PDO_LIVE_REQUIRED=1 but neither {} nor {} is set: this live test \
                 would have silently skipped",
                primary, fallback
            );
        }
        dsn
    }

    /// Reads a `*const c_char` bridge string return into an owned `String`.
    unsafe fn read(p: *const c_char) -> String {
        if p.is_null() {
            return String::new();
        }
        CStr::from_ptr(p).to_string_lossy().into_owned()
    }

    /// Builds an owned NUL-terminated C string for the extern-shaped string args.
    fn cs(s: &str) -> CString {
        CString::new(s).unwrap()
    }

    /// Reads a bridge raw-data pointer and length into owned bytes.
    unsafe fn read_bytes(p: *const c_char, len: i64) -> Vec<u8> {
        if p.is_null() || len <= 0 {
            return Vec::new();
        }
        std::slice::from_raw_parts(p as *const u8, len as usize).to_vec()
    }

    /// The ABI version constant tracks the current bridge surface; the per-version
    /// history is enumerated on `elephc_pdo_version`'s own docblock. v55 exposes
    /// complete PDO_INFORMIX column metadata on top of v54 open diagnostics.
    #[test]
    fn version_is_v55() {
        assert_eq!(elephc_pdo_version(), 55);
    }

    /// Connection-information accessors return empty strings for unknown handles.
    #[test]
    fn connection_information_is_empty_for_unknown_handle() {
        assert_eq!(unsafe { read(elephc_pdo_client_version(-999)) }, "");
        assert_eq!(unsafe { read(elephc_pdo_server_info(-999)) }, "");
        assert_eq!(unsafe { read(elephc_pdo_connection_status(-999)) }, "");
    }

    /// The rendered-SQL accessor is empty for an unknown statement handle.
    #[test]
    fn sent_sql_is_empty_for_unknown_stmt() {
        assert_eq!(unsafe { read(elephc_pdo_stmt_sent_sql(-999)) }, "");
    }

    /// The two v23 metadata accessors return their neutral "not a PostgreSQL
    /// column" answers for an unknown statement handle: an empty native type and
    /// the invalid OID `0`. (A real `pgsql:` column's non-empty name / non-zero
    /// OID is exercised by the live `#[ignore]` codegen tests, since a prepared
    /// PostgreSQL statement is needed to populate the column descriptor.)
    #[test]
    fn column_metadata_accessors_neutral_for_unknown_stmt() {
        let native = elephc_pdo_column_native_type(-999, 0);
        assert!(
            unsafe { std::ffi::CStr::from_ptr(native) }
                .to_bytes()
                .is_empty(),
            "native type for an unknown statement handle must be empty",
        );
        assert_eq!(
            elephc_pdo_column_type_oid(-999, 0),
            0,
            "type OID for an unknown statement handle must be the invalid OID 0",
        );
    }

    /// `elephc_pdo_in_transaction` reads SQLite's live autocommit state through
    /// the full C ABI: `0` before any `BEGIN`, `1` once `elephc_pdo_begin` starts
    /// one, `0` again after `elephc_pdo_commit` — the same live signal the
    /// codegen test `test_pdo_in_transaction_reflects_raw_begin` exercises at the
    /// PHP level via a raw `exec("BEGIN")` instead of `beginTransaction()`.
    #[test]
    fn sqlite_in_transaction_reflects_live_autocommit_state() {
        let dsn = cs("sqlite::memory:");
        let conn = unsafe { elephc_pdo_open(dsn.as_ptr()) };
        assert!(conn > 0, "open failed");
        assert_eq!(elephc_pdo_in_transaction(conn), 0, "no transaction yet");
        assert_eq!(elephc_pdo_begin(conn), 1);
        assert_eq!(elephc_pdo_in_transaction(conn), 1, "BEGIN must be visible live");
        assert_eq!(elephc_pdo_commit(conn), 1);
        assert_eq!(elephc_pdo_in_transaction(conn), 0, "COMMIT must clear it live");
        elephc_pdo_close(conn);
    }

    /// `elephc_pdo_in_transaction` reports `-1` ("unknown, use the caller's own
    /// flag") for a handle this bridge has never seen.
    #[test]
    fn in_transaction_unknown_handle_is_negative_one() {
        assert_eq!(elephc_pdo_in_transaction(-12345), -1);
    }

    /// MySQL and PostgreSQL transaction classifiers expose raw PDO::exec control
    /// statements while preserving savepoint and chained-transaction semantics.
    #[test]
    fn external_driver_transaction_state_tracks_control_sql() {
        assert!(my::transaction_state_after_sql("BEGIN", false, true));
        assert!(my::transaction_state_after_sql("ROLLBACK TO SAVEPOINT s", true, true));
        assert!(!my::transaction_state_after_sql("COMMIT", true, true));
        assert!(my::transaction_state_after_sql("INSERT INTO t VALUES (1)", false, false));
        assert!(!my::transaction_state_after_sql("CREATE TABLE t (n INT)", true, false));

        assert!(pg::transaction_state_after_sql("START TRANSACTION", false));
        assert!(pg::transaction_state_after_sql("ROLLBACK TO s", true));
        assert!(pg::transaction_state_after_sql("COMMIT AND CHAIN", true));
        assert!(!pg::transaction_state_after_sql("ROLLBACK", true));
    }

    /// A DSN for an unsupported driver is rejected with a driver error.
    #[test]
    fn open_rejects_unknown_driver_dsn() {
        let dsn = cs("oracle:host=localhost");
        let id = unsafe { elephc_pdo_open(dsn.as_ptr()) };
        assert_eq!(id, -1);
        let msg = unsafe { read(elephc_pdo_last_open_error()) };
        assert!(msg.contains("driver"), "got: {msg}");
    }

    /// Unknown handles return the documented sentinels rather than panicking.
    #[test]
    fn unknown_handles_return_sentinels() {
        assert_eq!(elephc_pdo_step(999_999), -1);
        assert_eq!(elephc_pdo_column_count(999_999), 0);
        assert_eq!(elephc_pdo_finalize(999_999), 0);
    }

    /// F-QUAL-02, half 1 (`ffi_guard`): the bridge's entry points are plain
    /// `extern "C"`, not `extern "C-unwind"`, so on rustc ≥ 1.81 a panic escaping one
    /// of them ABORTS the whole compiled PHP process — no catchable `PDOException`,
    /// no stack trace, just a dead program. A panic is not hypothetical here: an
    /// internal `unwrap`, a debug-build overflow, or an unexpected panic inside the
    /// `postgres`/`mysql` client crates all reach the boundary. `ffi_guard` converts
    /// any panic into the SAME "failed" answer each entry point's docblock already
    /// promises for an unknown handle (`-1`/`0`/`"00000"`/…), which the prelude turns
    /// into a normal error. Pinned here directly, since forcing a real panic inside a
    /// specific extern is not reproducible from a test.
    #[test]
    fn ffi_guard_converts_a_panic_into_the_documented_sentinel() {
        let minus_one = ffi_guard(-1_i64, || -> i64 {
            panic!("deliberate panic (F-QUAL-02 test) — expected");
        });
        assert_eq!(minus_one, -1);
        let zero = ffi_guard(0_i64, || -> i64 {
            panic!("deliberate panic (F-QUAL-02 test) — expected");
        });
        assert_eq!(zero, 0);
        // The happy path must still return the body's value untouched.
        assert_eq!(ffi_guard(-1_i64, || -> i64 { 7 }), 7);
        // The `*const c_char` entry points hand back a readable `'static` C string in
        // the panic path rather than a dangling/NULL pointer the prelude would deref.
        let state = ffi_guard(static_cstr(b"00000\0"), || -> *const c_char {
            panic!("deliberate panic (F-QUAL-02 test) — expected");
        });
        assert_eq!(unsafe { read(state) }, "00000");
    }

    /// F-QUAL-02, half 2 (`lock_recover`): a panic caught by [`ffi_guard`] while a
    /// handle-table lock was held leaves that `Mutex` POISONED for the life of the
    /// process. With the old `.lock().unwrap()` at all 84 lock sites, every later PDO
    /// call in that process — on unrelated connections, in unrelated request handlers
    /// — would then panic on the poisoned lock, and each of those panics would abort
    /// across the C ABI: one transient failure bricked the whole bridge. The tables
    /// are plain maps and stay structurally valid, so `lock_recover` reclaims the
    /// guard instead of re-panicking. This test poisons `conns()` for real (a thread
    /// that panics while holding it) and then proves the bridge still serves: the
    /// unknown-handle sentinels still come back, and a brand-new SQLite connection
    /// still opens, executes, steps and closes.
    ///
    /// The poison is process-global and deliberately NOT undone — that is the point:
    /// every other test in this binary keeps passing afterwards precisely because
    /// nothing reaches a poisoned lock through `.unwrap()` any more.
    #[test]
    fn poisoned_handle_table_does_not_brick_the_bridge() {
        let poisoner = std::thread::spawn(|| {
            // `lock_recover` (not `.lock().unwrap()`) so the test is robust whether or
            // not the table is already poisoned; the guard's drop during the unwind is
            // what sets the poison flag either way.
            let _guard = lock_recover(conns());
            panic!("deliberate poison of the conns() table (F-QUAL-02 test) — expected");
        });
        assert!(
            poisoner.join().is_err(),
            "the poisoning thread was supposed to panic"
        );
        assert!(
            conns().is_poisoned(),
            "precondition: the conns() mutex must actually be poisoned",
        );

        // Every entry point that reads the poisoned table still answers, and answers
        // with its documented sentinel rather than panicking (which would abort).
        let probe = cs("SELECT 1");
        assert_eq!(unsafe { elephc_pdo_exec(999_999, probe.as_ptr()) }, -1);
        assert_eq!(unsafe { elephc_pdo_prepare(999_999, probe.as_ptr(), 0) }, -1);
        assert_eq!(elephc_pdo_begin(999_999), 0);
        assert_eq!(unsafe { read(elephc_pdo_sqlstate(999_999)) }, "00000");

        // …and the bridge is still fully usable on a fresh connection.
        let dsn = cs("sqlite::memory:");
        let conn = unsafe { elephc_pdo_open(dsn.as_ptr()) };
        assert!(conn > 0, "open must still work through a poisoned conns()");
        let ddl = cs("CREATE TABLE t (n INTEGER)");
        assert_eq!(unsafe { elephc_pdo_exec(conn, ddl.as_ptr()) }, 0);
        let ins = cs("INSERT INTO t VALUES (42)");
        assert_eq!(unsafe { elephc_pdo_exec(conn, ins.as_ptr()) }, 1);
        let sel = cs("SELECT n FROM t");
        let stmt = unsafe { elephc_pdo_prepare(conn, sel.as_ptr(), 0) };
        assert!(stmt > 0, "prepare must still work through a poisoned conns()");
        assert_eq!(elephc_pdo_step(stmt), 1);
        assert_eq!(elephc_pdo_column_int(stmt, 0), 42);
        assert_eq!(elephc_pdo_finalize(stmt), 1);
        elephc_pdo_close(conn);
    }

    /// Full in-memory SQLite round-trip: open, create, insert, prepared select
    /// with a positional bind, step, and read typed columns back.
    #[test]
    fn sqlite_in_memory_round_trip() {
        let dsn = cs("sqlite::memory:");
        let conn = unsafe { elephc_pdo_open(dsn.as_ptr()) };
        assert!(conn > 0, "open failed");

        let ddl = cs("CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT, score REAL)");
        assert_eq!(unsafe { elephc_pdo_exec(conn, ddl.as_ptr()) }, 0);

        let ins = cs("INSERT INTO users (name, score) VALUES ('Alice', 9.5)");
        assert_eq!(unsafe { elephc_pdo_exec(conn, ins.as_ptr()) }, 1);
        assert_eq!(
            unsafe { elephc_pdo_last_insert_id(conn, std::ptr::null()) },
            1
        );

        let sql = cs("SELECT id, name, score FROM users WHERE id = ?");
        let stmt = unsafe { elephc_pdo_prepare(conn, sql.as_ptr(), 0) };
        assert!(stmt > 0, "prepare failed");
        assert_eq!(elephc_pdo_bind_int(stmt, 1, 1), 1);

        assert_eq!(elephc_pdo_step(stmt), 1);
        assert_eq!(elephc_pdo_column_count(stmt), 3);
        assert_eq!(elephc_pdo_column_int(stmt, 0), 1);
        assert_eq!(unsafe { read(elephc_pdo_column_name(stmt, 1)) }, "name");
        // v24/F-QUAL-03: the NUL-stripping `elephc_pdo_column_text` is gone; a text
        // column is read through the same byte-exact len+ptr pair the prelude uses.
        let name_len = elephc_pdo_column_data_len(stmt, 1);
        let name_ptr = elephc_pdo_column_data_ptr(stmt, 1);
        assert_eq!(unsafe { read_bytes(name_ptr, name_len) }, b"Alice");
        assert_eq!(elephc_pdo_column_double(stmt, 2), 9.5);
        assert_eq!(elephc_pdo_step(stmt), 0);

        assert_eq!(elephc_pdo_finalize(stmt), 1);
        elephc_pdo_close(conn);
    }

    /// SQLite BLOB data returned through the raw data API preserves embedded NUL
    /// bytes instead of truncating through the legacy C-string bridge.
    #[test]
    fn sqlite_blob_round_trip_preserves_embedded_nul() {
        let dsn = cs("sqlite::memory:");
        let conn = unsafe { elephc_pdo_open(dsn.as_ptr()) };
        assert!(conn > 0, "open failed");

        let ddl = cs("CREATE TABLE blobs (data BLOB)");
        assert_eq!(unsafe { elephc_pdo_exec(conn, ddl.as_ptr()) }, 0);

        let ins = cs("INSERT INTO blobs (data) VALUES (x'410042')");
        assert_eq!(unsafe { elephc_pdo_exec(conn, ins.as_ptr()) }, 1);

        let sql = cs("SELECT data FROM blobs");
        let stmt = unsafe { elephc_pdo_prepare(conn, sql.as_ptr(), 0) };
        assert!(stmt > 0, "prepare failed");
        assert_eq!(elephc_pdo_step(stmt), 1);
        assert_eq!(elephc_pdo_column_type(stmt, 0), 4);
        assert_eq!(elephc_pdo_column_data_len(stmt, 0), 3);
        let ptr = elephc_pdo_column_data_ptr(stmt, 0);
        assert_eq!(unsafe { read_bytes(ptr, 3) }, b"A\0B");
        assert_eq!(elephc_pdo_column_data_byte(stmt, 0, 0), 65);
        assert_eq!(elephc_pdo_column_data_byte(stmt, 0, 1), 0);
        assert_eq!(elephc_pdo_column_data_byte(stmt, 0, 2), 66);
        assert_eq!(elephc_pdo_column_data_byte(stmt, 0, 3), 0);

        assert_eq!(elephc_pdo_finalize(stmt), 1);
        elephc_pdo_close(conn);
    }

    /// F-QUAL-01 (ABI v24), bridge side: `elephc_pdo_blob_data_ptr` is the BULK
    /// copy-out path `blobStream()` now uses — one FFI call for the whole value,
    /// instead of one `elephc_pdo_blob_byte` call per byte (each locking and
    /// unlocking the handle table). Two properties are load-bearing and pinned here.
    /// (1) It is byte-exact: the buffer is length-counted and never routed through
    /// the NUL-stripping `store_cstr`, so `x'610062'` ("a\0b") survives whole — the
    /// only reason the byte loop existed. (2) It returns a NULL pointer when the
    /// buffer is EMPTY (a zero-length BLOB, `x''`), which is exactly why the prelude
    /// guards its `ptr_read_string` call with `if ($_len > 0)`: `ptr_read_string`
    /// runs `__rt_ptr_check_nonnull` BEFORE it ever looks at the length, so an empty
    /// BLOB reaching it would hard-abort the process rather than yield `""`.
    #[test]
    fn sqlite_blob_data_ptr_is_byte_exact_and_null_when_empty() {
        let dsn = cs("sqlite::memory:");
        let conn = unsafe { elephc_pdo_open(dsn.as_ptr()) };
        assert!(conn > 0, "open failed");

        let ddl = cs("CREATE TABLE imgs (id INTEGER PRIMARY KEY, body BLOB)");
        assert_eq!(unsafe { elephc_pdo_exec(conn, ddl.as_ptr()) }, 0);
        let ins = cs("INSERT INTO imgs (id, body) VALUES (1, x'610062'), (2, x'')");
        assert_eq!(unsafe { elephc_pdo_exec(conn, ins.as_ptr()) }, 2);

        let table = cs("imgs");
        let column = cs("body");
        let len = unsafe {
            elephc_pdo_blob_read(conn, table.as_ptr(), column.as_ptr(), 1, std::ptr::null())
        };
        assert_eq!(len, 3);
        let ptr = elephc_pdo_blob_data_ptr();
        assert!(!ptr.is_null(), "a non-empty BLOB must expose its buffer");
        assert_eq!(unsafe { read_bytes(ptr, len) }, b"a\0b");

        // v40 writeback is length-counted as well: an embedded NUL must not
        // truncate the replacement snapshot at the C ABI boundary.
        let replacement = b"Z\0Q";
        assert_eq!(
            unsafe {
                elephc_pdo_blob_write(
                    conn,
                    table.as_ptr(),
                    column.as_ptr(),
                    1,
                    std::ptr::null(),
                    replacement.as_ptr() as *const c_char,
                    replacement.len() as i64,
                )
            },
            1,
        );
        let replaced = unsafe {
            elephc_pdo_blob_read(conn, table.as_ptr(), column.as_ptr(), 1, std::ptr::null())
        };
        assert_eq!(replaced, 3);
        assert_eq!(
            unsafe { read_bytes(elephc_pdo_blob_data_ptr(), replaced) },
            replacement,
        );

        // v46 performs the same operation in bounded slices: size is scalar-only,
        // reads copy only the requested range, and writes patch only that range.
        assert_eq!(
            unsafe {
                elephc_pdo_blob_size(
                    conn,
                    table.as_ptr(),
                    column.as_ptr(),
                    1,
                    std::ptr::null(),
                )
            },
            3,
        );
        let slice_len = unsafe {
            elephc_pdo_blob_read_at(
                conn,
                table.as_ptr(),
                column.as_ptr(),
                1,
                std::ptr::null(),
                1,
                1,
            )
        };
        assert_eq!(slice_len, 1);
        assert_eq!(unsafe { read_bytes(elephc_pdo_blob_data_ptr(), slice_len) }, b"\0");
        let patch = b"X";
        assert_eq!(
            unsafe {
                elephc_pdo_blob_write_at(
                    conn,
                    table.as_ptr(),
                    column.as_ptr(),
                    1,
                    std::ptr::null(),
                    1,
                    patch.as_ptr() as *const c_char,
                    patch.len() as i64,
                )
            },
            1,
        );
        let patched = unsafe {
            elephc_pdo_blob_read(conn, table.as_ptr(), column.as_ptr(), 1, std::ptr::null())
        };
        assert_eq!(unsafe { read_bytes(elephc_pdo_blob_data_ptr(), patched) }, b"ZXQ");
        assert_eq!(
            unsafe {
                elephc_pdo_blob_write_at(
                    conn,
                    table.as_ptr(),
                    column.as_ptr(),
                    1,
                    std::ptr::null(),
                    3,
                    patch.as_ptr() as *const c_char,
                    1,
                )
            },
            -1,
            "bounded writes must not extend SQLite's fixed-size BLOB",
        );

        // The zero-length BLOB: a successful read of 0 bytes, and a NULL data pointer.
        let empty = unsafe {
            elephc_pdo_blob_read(conn, table.as_ptr(), column.as_ptr(), 2, std::ptr::null())
        };
        assert_eq!(empty, 0, "a zero-length BLOB reads successfully, as 0 bytes");
        assert!(
            elephc_pdo_blob_data_ptr().is_null(),
            "an empty buffer must report NULL, which is what the prelude's `$_len > 0` guard is for",
        );

        // A missing row is still the -1 failure sentinel, distinct from "0 bytes".
        let missing = unsafe {
            elephc_pdo_blob_read(conn, table.as_ptr(), column.as_ptr(), 999, std::ptr::null())
        };
        assert_eq!(missing, -1);

        elephc_pdo_close(conn);
    }

    /// F-SQLT-02 (ABI v24), bridge side: `Pdo\Sqlite::ATTR_EXTENDED_RESULT_CODES` was
    /// stored and otherwise a total no-op. php-src's `pdo_sqlite_set_attribute` calls
    /// `sqlite3_extended_result_codes(H->db, lval)`, which widens the value
    /// `sqlite3_errcode()` reports — and which PDO surfaces as `errorInfo[1]` — from
    /// the coarse primary code (`SQLITE_CONSTRAINT` = 19, "a constraint broke") to the
    /// extended one naming the constraint (`SQLITE_CONSTRAINT_UNIQUE` = 2067). The
    /// attribute is a live toggle, so turning it back off must restore the primary
    /// code; a non-SQLite/unknown handle answers `0` (no-op), never panics.
    #[test]
    fn sqlite_extended_result_codes_widen_the_native_errcode() {
        let dsn = cs("sqlite::memory:");
        let conn = unsafe { elephc_pdo_open(dsn.as_ptr()) };
        assert!(conn > 0, "open failed");

        let ddl = cs("CREATE TABLE t (id INTEGER PRIMARY KEY, u TEXT UNIQUE)");
        assert_eq!(unsafe { elephc_pdo_exec(conn, ddl.as_ptr()) }, 0);
        let seed = cs("INSERT INTO t (id, u) VALUES (1, 'a')");
        assert_eq!(unsafe { elephc_pdo_exec(conn, seed.as_ptr()) }, 1);

        // Off (the SQLite default): the primary code, SQLITE_CONSTRAINT.
        let dup2 = cs("INSERT INTO t (id, u) VALUES (2, 'a')");
        assert_eq!(unsafe { elephc_pdo_exec(conn, dup2.as_ptr()) }, -1);
        assert_eq!(elephc_pdo_errcode(conn), 19);

        // On: the extended code, SQLITE_CONSTRAINT_UNIQUE (19 | 8 << 8).
        assert_eq!(elephc_pdo_set_extended_result_codes(conn, 1), 1);
        let dup3 = cs("INSERT INTO t (id, u) VALUES (3, 'a')");
        assert_eq!(unsafe { elephc_pdo_exec(conn, dup3.as_ptr()) }, -1);
        assert_eq!(elephc_pdo_errcode(conn), 2067);

        // Off again: back to the primary code (it is a toggle, not a latch).
        assert_eq!(elephc_pdo_set_extended_result_codes(conn, 0), 1);
        let dup4 = cs("INSERT INTO t (id, u) VALUES (4, 'a')");
        assert_eq!(unsafe { elephc_pdo_exec(conn, dup4.as_ptr()) }, -1);
        assert_eq!(elephc_pdo_errcode(conn), 19);

        elephc_pdo_close(conn);

        // A handle that is not a live SQLite connection is a no-op, not a panic.
        assert_eq!(elephc_pdo_set_extended_result_codes(999_999, 1), 0);
    }

    /// F-QUAL-04: `SqliteConn`/`SqliteStmt` now carry `impl Drop` (sqlite3_close /
    /// sqlite3_finalize) as a defense-in-depth net, since their safety used to rest
    /// entirely on the two explicit `close()`/`finalize()` call sites never being
    /// missed. The net's own risk is the mirror image — a DOUBLE free: `Drop` runs
    /// when the handle leaves the table, right after the explicit release already ran
    /// on the very same raw pointer, and `sqlite3_finalize`/`sqlite3_close` on an
    /// already-destroyed handle is undefined behavior (in practice an abort or heap
    /// corruption, not a clean error). The releases are therefore guarded by a
    /// `released` flag, which this test drives from the outside: every normal path
    /// releases a handle exactly ONCE, a second explicit release is a reported no-op
    /// (`0`, "unknown handle" — it was already removed from the table), and the
    /// process survives to answer the next call.
    #[test]
    fn sqlite_close_and_finalize_are_idempotent_under_the_drop_net() {
        let dsn = cs("sqlite::memory:");
        let conn = unsafe { elephc_pdo_open(dsn.as_ptr()) };
        assert!(conn > 0, "open failed");

        let sql = cs("SELECT 1");
        let stmt = unsafe { elephc_pdo_prepare(conn, sql.as_ptr(), 0) };
        assert!(stmt > 0, "prepare failed");
        assert_eq!(elephc_pdo_step(stmt), 1);

        // Explicit release, then Drop on the way out of the table: exactly one free.
        assert_eq!(elephc_pdo_finalize(stmt), 1);
        assert_eq!(
            elephc_pdo_finalize(stmt),
            0,
            "a second finalize must be an unknown-handle no-op, not a second free",
        );

        elephc_pdo_close(conn);
        // A second close must not re-enter sqlite3_close on the freed handle.
        elephc_pdo_close(conn);
        assert_eq!(
            unsafe { elephc_pdo_exec(conn, sql.as_ptr()) },
            -1,
            "the closed connection's handle must no longer resolve",
        );
    }

    /// SQLite persistent opens reuse a process-local connection keyed by the
    /// `(DSN, persistent-key)` pair — here the empty key both opens pass, i.e. the
    /// plain boolean-persistent pool (F-CORE-16) — and a close call leaves that
    /// pooled connection available to the next open.
    #[test]
    fn sqlite_persistent_pool_reuses_connection_after_close() {
        let dsn = cs("sqlite::memory:");
        let first = unsafe {
            elephc_pdo_open_persistent(
                dsn.as_ptr(),
                1,
                0,
                std::ptr::null(),
                std::ptr::null(),
                0,
                std::ptr::null(),
                std::ptr::null(),
            )
        };
        assert!(first > 0, "open failed");

        let ddl = cs("CREATE TABLE persistent_pool (n INTEGER)");
        assert_eq!(unsafe { elephc_pdo_exec(first, ddl.as_ptr()) }, 0);
        let ins = cs("INSERT INTO persistent_pool VALUES (77)");
        assert_eq!(unsafe { elephc_pdo_exec(first, ins.as_ptr()) }, 1);
        elephc_pdo_close(first);

        let second = unsafe {
            elephc_pdo_open_persistent(
                dsn.as_ptr(),
                1,
                0,
                std::ptr::null(),
                std::ptr::null(),
                0,
                std::ptr::null(),
                std::ptr::null(),
            )
        };
        assert_eq!(second, first);
        let sql = cs("SELECT n FROM persistent_pool");
        let stmt = unsafe { elephc_pdo_prepare(second, sql.as_ptr(), 0) };
        assert!(stmt > 0, "prepare failed");
        assert_eq!(elephc_pdo_step(stmt), 1);
        assert_eq!(elephc_pdo_column_int(stmt, 0), 77);
        assert_eq!(elephc_pdo_finalize(stmt), 1);
    }

    /// ABI v44 tracks simultaneous PDO owners of one persistent handle and only
    /// marks the pooled session idle after the final release.
    #[test]
    fn persistent_release_counts_live_owners() {
        let dsn = cs("sqlite::memory:");
        let key = cs("v44-owner-count");
        let first = unsafe {
            elephc_pdo_open_persistent(
                dsn.as_ptr(),
                1,
                0,
                std::ptr::null(),
                std::ptr::null(),
                0,
                key.as_ptr(),
                std::ptr::null(),
            )
        };
        let second = unsafe {
            elephc_pdo_open_persistent(
                dsn.as_ptr(),
                1,
                0,
                std::ptr::null(),
                std::ptr::null(),
                0,
                key.as_ptr(),
                std::ptr::null(),
            )
        };
        assert_eq!(first, second);
        assert_eq!(lock_recover(persistent_owner_counts()).get(&first), Some(&2));

        elephc_pdo_release(first, 1);
        assert_eq!(lock_recover(persistent_owner_counts()).get(&first), Some(&1));
        elephc_pdo_release(second, 1);
        assert_eq!(lock_recover(persistent_owner_counts()).get(&first), Some(&0));

        let reused = unsafe {
            elephc_pdo_open_persistent(
                dsn.as_ptr(),
                1,
                0,
                std::ptr::null(),
                std::ptr::null(),
                0,
                key.as_ptr(),
                std::ptr::null(),
            )
        };
        assert_eq!(reused, first);
        assert_eq!(lock_recover(persistent_owner_counts()).get(&first), Some(&1));
        elephc_pdo_release(reused, 1);
    }

    /// F-CORE-16: the persistent pool key is the `(DSN, persistent-key)` PAIR, not the
    /// DSN alone. php-src builds its persistent hashkey from both whenever
    /// `PDO::ATTR_PERSISTENT` was given as a non-numeric, non-empty string
    /// (`pdo_dbh.c:389-404`) — separating two named pools onto distinct connections is
    /// the entire point of that spelling — and elephc used to cast the option to `(bool)`
    /// and pool by DSN alone, so two differently-named persistent pools silently SHARED
    /// one connection.
    ///
    /// Driven through the C ABI exactly as the pooled-reuse test above drives it, with
    /// `persistent_key` (the v25 trailing parameter) as the only difference between the
    /// opens. Distinct handle IDs alone would be a weak assertion — the pool could hand
    /// back two IDs aliasing one `sqlite3*` — so distinctness is PROVEN at the database
    /// level: `sqlite::memory:` gives each real connection its own private in-memory
    /// database, so the same `CREATE TABLE` must SUCCEED on both (`0` rows affected).
    /// Were they one shared connection, the second `CREATE TABLE` would fail with "table
    /// already exists" and `elephc_pdo_exec` would return `-1`.
    ///
    /// The key strings are unique to this test, so it cannot collide with the
    /// process-global pool entries any other test in this binary registers (they all use
    /// the empty key).
    #[test]
    fn sqlite_persistent_pool_key_includes_the_attr_persistent_string() {
        let dsn = cs("sqlite::memory:");
        let key_alpha = cs("fcore16-alpha");
        let key_beta = cs("fcore16-beta");

        let alpha = unsafe {
            elephc_pdo_open_persistent(
                dsn.as_ptr(),
                1,
                0,
                std::ptr::null(),
                std::ptr::null(),
                0,
                key_alpha.as_ptr(),
                std::ptr::null(),
            )
        };
        assert!(alpha > 0, "open under key alpha failed");

        let beta = unsafe {
            elephc_pdo_open_persistent(
                dsn.as_ptr(),
                1,
                0,
                std::ptr::null(),
                std::ptr::null(),
                0,
                key_beta.as_ptr(),
                std::ptr::null(),
            )
        };
        assert!(beta > 0, "open under key beta failed");
        assert_ne!(
            alpha, beta,
            "same DSN under DIFFERENT ATTR_PERSISTENT keys must be DISTINCT pooled \
             connections (php-src pdo_dbh.c:389-404)",
        );

        // Same DDL on both: it can only succeed twice if these are two real, separate
        // `sqlite::memory:` databases rather than one connection behind two handle IDs.
        let ddl = cs("CREATE TABLE keyed_pool (n INTEGER)");
        assert_eq!(
            unsafe { elephc_pdo_exec(alpha, ddl.as_ptr()) },
            0,
            "DDL on the alpha-keyed connection failed",
        );
        assert_eq!(
            unsafe { elephc_pdo_exec(beta, ddl.as_ptr()) },
            0,
            "the beta-keyed connection shares alpha's database — the pool key ignored the \
             ATTR_PERSISTENT string",
        );

        // And the SAME key still pools: a second open under alpha's key reuses alpha's
        // handle rather than dialing a fresh connection (the reuse half of the pair key).
        let alpha_again = unsafe {
            elephc_pdo_open_persistent(
                dsn.as_ptr(),
                1,
                0,
                std::ptr::null(),
                std::ptr::null(),
                0,
                key_alpha.as_ptr(),
                std::ptr::null(),
            )
        };
        assert_eq!(
            alpha_again, alpha,
            "the SAME (DSN, persistent-key) pair must reuse the pooled connection",
        );
    }

    /// F-MY-02: `unix_socket` is only honored when the DSN names NO host, or names
    /// exactly `localhost`. php-src's MySQL handle factory takes the socket under
    /// precisely that condition — `if (vars[0].optval && !strcmp("localhost",
    /// vars[0].optval))` (`mysql_driver.c:940-946`), with the DSN parser defaulting an
    /// absent `host` to `"localhost"` — so `mysql:host=127.0.0.1;unix_socket=…` is
    /// TCP-only in real PHP and the socket key is silently ignored. Preferring the socket
    /// whenever it appeared (as `build_opts` did) connected such a DSN to a DIFFERENT
    /// SERVER than php-src would: same DSN, different database.
    ///
    /// `127.0.0.1` is deliberately NOT `localhost` here — the comparison is php-src's
    /// case-sensitive `strcmp`, and the MySQL client itself draws the same distinction.
    /// Pure DSN-parsing logic: `build_opts` never dials out, so no server is needed.
    /// (It lives in this `mod tests` rather than `my.rs`'s only because of how this
    /// change was split across owners; its subject is `my::build_opts`.)
    #[test]
    fn build_opts_ignores_unix_socket_when_the_host_is_a_real_address() {
        let (opts, _charset) = crate::my::build_opts(
            "mysql:host=127.0.0.1;port=3307;unix_socket=/tmp/mysql.sock;dbname=testdb",
            false,
            false,
        )
        .expect("build_opts rejected a valid mysql: DSN");
        let opts: mysql::Opts = opts.into();
        assert_eq!(
            opts.get_socket(),
            None,
            "a non-localhost host must force the TCP path and drop the socket key",
        );
        assert_eq!(opts.get_ip_or_hostname(), "127.0.0.1");
        assert_eq!(opts.get_tcp_port(), 3307);
    }

    /// F-MY-03: the NO_BACKSLASH_ESCAPES-aware placeholder scanner. Under that `sql_mode`
    /// the SERVER treats `\` as an ORDINARY BYTE inside a string literal — doubling is then
    /// the only escape — so the scanner has to agree with it about where a literal ENDS.
    ///
    /// `SELECT 'it\', ? FROM t` is the shape that makes the disagreement observable, and it
    /// flips the PLACEHOLDER COUNT, not some cosmetic detail:
    ///   * NBE **true** — the `\` is a literal byte, so the `'` right after it CLOSES the
    ///     string. The literal is `'it\'` and the `?` that follows is a REAL placeholder:
    ///     1 slot.
    ///   * NBE **false** — the `\` escapes the `'`, so the literal does NOT end there; the
    ///     scanner continues to the quote before `tail` and SWALLOWS the `?` as string
    ///     content: 0 slots.
    ///
    /// Assuming backslash-escaping under NBE therefore yields a bound-parameter count that
    /// disagrees with the server's real one — precisely what this flag exists to prevent.
    /// Pure scanning logic: `translate_placeholders` never dials out, so no server is needed.
    /// (Like the `build_opts` tests around it, its subject is `my`; it lives in this
    /// `mod tests` only because of how this change was split across owners.)
    #[test]
    fn translate_placeholders_honors_no_backslash_escapes() {
        // Rust `\\` is ONE literal backslash. The quote after it closes under NBE;
        // under the default mode it is escaped and the quote before tail closes instead.
        let sql = "SELECT 'it\\', ?, 'tail' FROM t";

        let (_sql, named, order, mixed) = crate::my::translate_placeholders(sql, true);
        assert_eq!(
            order.len(),
            1,
            "under NO_BACKSLASH_ESCAPES the backslash is a literal byte, so the string \
             closes at the following quote and the trailing ? is a real placeholder",
        );
        assert!(named.is_empty(), "there are no :name placeholders in this SQL");
        assert!(!mixed, "a lone positional ? must not read as mixed named/positional");

        let (_sql, _named, order, _mixed) = crate::my::translate_placeholders(sql, false);
        assert_eq!(
            order.len(),
            0,
            "with backslash-escaping the \\' does NOT close the literal, so the ? is \
             swallowed as string content",
        );
    }

    /// F-MY-03, the negative control: the flag must change ONLY the backslash rule. A
    /// placeholder outside any literal, and the doubled-quote escape (`''`, an escape in
    /// BOTH modes), behave identically whichever way the flag is set. Without this, the
    /// test above would also pass for a scanner that simply mis-scans under one mode.
    #[test]
    fn translate_placeholders_is_otherwise_unchanged_by_no_backslash_escapes() {
        // `'it''s'` is a doubled-quote escape in both modes: the literal ends at its final
        // quote, and BOTH `?` placeholders are real.
        let sql = "SELECT ? FROM t WHERE a = 'it''s' AND b = ?";
        for nbe in [false, true] {
            let (_sql, named, order, mixed) = crate::my::translate_placeholders(sql, nbe);
            assert_eq!(
                order.len(),
                2,
                "doubled-quote escaping is mode-independent (no_backslash_escapes={nbe})",
            );
            assert!(named.is_empty());
            assert!(!mixed);
        }
    }

    /// F-MY-02, the two cases where the socket DOES win: `host=localhost` (php-src's
    /// literal `strcmp` match) and a DSN naming no host at all (php-src's parser defaults
    /// `host` to `"localhost"`, so it takes the same arm). Without this negative control,
    /// a `build_opts` that simply never honored `unix_socket` would pass the test above.
    #[test]
    fn build_opts_honors_unix_socket_for_localhost_and_for_a_hostless_dsn() {
        let (opts, _charset) = crate::my::build_opts(
            "mysql:host=localhost;unix_socket=/tmp/mysql.sock;dbname=testdb",
            false,
            false,
        )
        .expect("build_opts rejected a valid mysql: DSN");
        let opts: mysql::Opts = opts.into();
        assert_eq!(
            opts.get_socket(),
            Some("/tmp/mysql.sock"),
            "host=localhost must take the unix_socket path",
        );

        let (opts, _charset) =
            crate::my::build_opts(
                "mysql:unix_socket=/tmp/mysql.sock;dbname=testdb",
                false,
                false,
            )
                .expect("build_opts rejected a valid mysql: DSN");
        let opts: mysql::Opts = opts.into();
        assert_eq!(
            opts.get_socket(),
            Some("/tmp/mysql.sock"),
            "a host-less DSN defaults to localhost, so it too must take the socket",
        );
    }

    /// F-MY-06: `Pdo\Mysql::ATTR_FOUND_ROWS` ORs `CLIENT_FOUND_ROWS` into the connect
    /// handshake's capability flags (php-src `mysql_driver.c:776-778`), which switches
    /// what the server reports as an UPDATE's affected-row count — and so
    /// `PDOStatement::rowCount()` — from "rows actually CHANGED" to "rows MATCHED by the
    /// WHERE clause". It is a HANDSHAKE capability, so it can only be selected at connect
    /// time; it was unwired entirely, leaving no way to opt into the matched-rows
    /// semantics apps commonly rely on (the difference between `1` and `0` for an UPDATE
    /// writing a value a row already holds).
    ///
    /// Asserted on the BUILT `Opts` — the capability is a bit in the options, so no
    /// server is needed to prove it is set iff requested. The `mysql` crate ORs
    /// `additional_capabilities` into the handshake's client flags, and its
    /// forbidden-flag filter covers only the capabilities the connection manages itself
    /// (`CLIENT_SSL`, `CLIENT_COMPRESS`, the MULTI_* pair, …) — never `CLIENT_FOUND_ROWS`
    /// — so a bit present here does reach the server.
    #[test]
    fn build_opts_sets_client_found_rows_only_when_requested() {
        let (opts, _charset) =
            crate::my::build_opts("mysql:host=localhost;dbname=testdb", true, false)
            .expect("build_opts rejected a valid mysql: DSN");
        let opts: mysql::Opts = opts.into();
        assert!(
            opts.get_additional_capabilities()
                .contains(mysql::consts::CapabilityFlags::CLIENT_FOUND_ROWS),
            "ATTR_FOUND_ROWS must OR CLIENT_FOUND_ROWS into the handshake capabilities",
        );

        let (opts, _charset) =
            crate::my::build_opts("mysql:host=localhost;dbname=testdb", false, false)
            .expect("build_opts rejected a valid mysql: DSN");
        let opts: mysql::Opts = opts.into();
        assert!(
            !opts
                .get_additional_capabilities()
                .contains(mysql::consts::CapabilityFlags::CLIENT_FOUND_ROWS),
            "without ATTR_FOUND_ROWS the capability must stay off (php-src's default: an \
             UPDATE's rowCount() reports rows CHANGED)",
        );
    }

    /// F-CORE-02: pins the LAST-KEY-WINS property of `build_opts`'s DSN pair loop, which
    /// is the mechanism the prelude's MySQL credential precedence is built on and which
    /// nothing else in CI covers.
    ///
    /// php-src is asymmetric by driver: for `pgsql:` the DSN wins, but for `mysql:` the
    /// CONSTRUCTOR ARGUMENTS win and the DSN's own `user=`/`password=` are only a fallback
    /// (`mysql_driver.c:948-953`). The prelude implements that by APPENDING `;user=…` /
    /// `;password=…` to the DSN for a `mysql:` connection (`src/pdo_prelude.rs:691-698`) —
    /// which overrides the DSN's keys only because the loop above reassigns `user`/
    /// `password` on every occurrence, so the trailing pair is the one that survives.
    ///
    /// That coupling is invisible from either side alone: rewriting the loop to take the
    /// FIRST occurrence (`user.get_or_insert(value)`) would still pass every other test
    /// here while silently restoring the pre-F-CORE-02 bug — `new PDO("mysql:…;user=
    /// readonly", "admin", $pw)` connecting as `readonly`. The only other coverage is a
    /// live `#[ignore]` test that never runs in CI, so this asserts it on the built `Opts`,
    /// where no server is needed to prove which credentials would be sent.
    #[test]
    fn build_opts_lets_the_last_user_and_password_keys_win() {
        let (opts, _charset) = crate::my::build_opts(
            "mysql:host=localhost;dbname=testdb;user=readonly;password=weak;user=admin;\
             password=strong",
            false,
            false,
        )
        .expect("build_opts rejected a valid mysql: DSN");
        let opts: mysql::Opts = opts.into();
        assert_eq!(
            opts.get_user(),
            Some("admin"),
            "the trailing user= key (the prelude's appended constructor argument) must win \
             over the one already in the DSN",
        );
        assert_eq!(
            opts.get_pass(),
            Some("strong"),
            "the trailing password= key (the prelude's appended constructor argument) must \
             win over the one already in the DSN",
        );
    }

    /// v7 SQLite coverage: a clean DDL/INSERT reports SQLSTATE `"00000"`;
    /// `elephc_pdo_bind_bool` and `elephc_pdo_bind_blob` round-trip through
    /// `column_int`/`column_data_ptr` (the blob preserving an embedded NUL byte);
    /// `elephc_pdo_set_busy_timeout` reports success; `elephc_pdo_server_version`
    /// returns the bundled SQLite version string; and a duplicate PRIMARY KEY
    /// insert reports SQLSTATE `"23000"` (SQLite's `SQLITE_CONSTRAINT`).
    #[test]
    fn sqlite_v7_sqlstate_and_new_binds() {
        let dsn = cs("sqlite::memory:");
        let conn = unsafe { elephc_pdo_open(dsn.as_ptr()) };
        assert!(conn > 0, "open failed");

        assert_eq!(elephc_pdo_set_busy_timeout(conn, 5000), 1);

        let version = unsafe { read(elephc_pdo_server_version(conn)) };
        assert!(!version.is_empty(), "server_version was empty");
        assert!(
            version.chars().next().is_some_and(|c| c.is_ascii_digit()),
            "got: {version}"
        );

        let ddl = cs("CREATE TABLE t (id INTEGER PRIMARY KEY, flag INTEGER, data BLOB)");
        assert_eq!(unsafe { elephc_pdo_exec(conn, ddl.as_ptr()) }, 0);
        assert_eq!(unsafe { read(elephc_pdo_sqlstate(conn)) }, "00000");

        let ins = cs("INSERT INTO t (id, flag, data) VALUES (?, ?, ?)");
        let stmt = unsafe { elephc_pdo_prepare(conn, ins.as_ptr(), 0) };
        assert!(stmt > 0, "prepare failed");
        assert_eq!(elephc_pdo_bind_int(stmt, 1, 1), 1);
        assert_eq!(elephc_pdo_bind_bool(stmt, 2, 1), 1);
        let blob = b"A\0B";
        assert_eq!(
            unsafe {
                elephc_pdo_bind_blob(stmt, 3, blob.as_ptr() as *const c_char, blob.len() as i64)
            },
            1
        );
        assert_eq!(elephc_pdo_step(stmt), 0);
        assert_eq!(elephc_pdo_finalize(stmt), 1);
        assert_eq!(unsafe { read(elephc_pdo_sqlstate(conn)) }, "00000");
        assert_eq!(
            unsafe { read(elephc_pdo_last_insert_id_text(conn, std::ptr::null())) },
            "1"
        );

        // The bound bool (as 0/1) and blob (with its embedded NUL) round-trip.
        let sel = cs("SELECT flag, data FROM t WHERE id = 1");
        let q = unsafe { elephc_pdo_prepare(conn, sel.as_ptr(), 0) };
        assert!(q > 0, "prepare failed");
        assert_eq!(elephc_pdo_step(q), 1);
        assert_eq!(elephc_pdo_column_int(q, 0), 1);
        assert_eq!(elephc_pdo_column_data_len(q, 1), 3);
        let ptr = elephc_pdo_column_data_ptr(q, 1);
        assert_eq!(unsafe { read_bytes(ptr, 3) }, b"A\0B");
        assert_eq!(elephc_pdo_finalize(q), 1);

        // A duplicate PRIMARY KEY hits SQLite's SQLITE_CONSTRAINT, SQLSTATE
        // "23000", visible through both the connection- and statement-level
        // accessors right after the failing step. (SQLite only guarantees
        // `sqlite3_errcode()` reflects the *most recently failed* call, so this
        // reads it immediately rather than after any later successful call.)
        let dup = cs("INSERT INTO t (id, flag, data) VALUES (?, 0, NULL)");
        let dup_stmt = unsafe { elephc_pdo_prepare(conn, dup.as_ptr(), 0) };
        assert!(dup_stmt > 0, "prepare failed");
        assert_eq!(elephc_pdo_bind_int(dup_stmt, 1, 1), 1);
        assert_eq!(elephc_pdo_step(dup_stmt), -1);
        assert_eq!(unsafe { read(elephc_pdo_sqlstate(conn)) }, "23000");
        assert_eq!(unsafe { read(elephc_pdo_stmt_sqlstate(dup_stmt)) }, "23000");
        assert_eq!(elephc_pdo_stmt_errcode(dup_stmt), elephc_pdo_errcode(conn));
        assert_eq!(
            unsafe { read(elephc_pdo_stmt_errmsg(dup_stmt)) },
            unsafe { read(elephc_pdo_errmsg(conn)) }
        );
        assert_eq!(elephc_pdo_finalize(dup_stmt), 1);

        elephc_pdo_close(conn);
    }

    /// v17 (P1-10): `Pdo\Sqlite::ATTR_OPEN_FLAGS` threaded through
    /// `elephc_pdo_open_persistent`'s `sqlite_open_flags` opens a connection that
    /// rejects writes when the flags select `SQLITE_OPEN_READONLY` (`1`, matching
    /// `Pdo\Sqlite::OPEN_READONLY`) instead of the default `READWRITE|CREATE`.
    #[test]
    fn sqlite_open_flags_readonly_rejects_write() {
        let path = std::env::temp_dir().join(format!(
            "elephc_pdo_test_readonly_{}_{}.sqlite",
            std::process::id(),
            next_id()
        ));
        let _ = std::fs::remove_file(&path);
        let dsn = cs(&format!("sqlite:{}", path.display()));

        // A non-persistent (flag 0) open with sqlite_open_flags=0 creates the file
        // read-write, as today's default does.
        let rw = unsafe {
            elephc_pdo_open_persistent(
                dsn.as_ptr(),
                0,
                0,
                std::ptr::null(),
                std::ptr::null(),
                0,
                std::ptr::null(),
                std::ptr::null(),
            )
        };
        assert!(rw > 0, "read-write open failed");
        let ddl = cs("CREATE TABLE t (n INTEGER)");
        assert_eq!(unsafe { elephc_pdo_exec(rw, ddl.as_ptr()) }, 0);
        elephc_pdo_close(rw);

        // Reopening with sqlite_open_flags=1 (SQLITE_OPEN_READONLY) must reject a
        // write against the now-existing file.
        let ro = unsafe {
            elephc_pdo_open_persistent(
                dsn.as_ptr(),
                0,
                1,
                std::ptr::null(),
                std::ptr::null(),
                0,
                std::ptr::null(),
                std::ptr::null(),
            )
        };
        assert!(ro > 0, "read-only open failed");
        let ins = cs("INSERT INTO t VALUES (1)");
        assert_eq!(
            unsafe { elephc_pdo_exec(ro, ins.as_ptr()) },
            -1,
            "write should be rejected on a read-only handle"
        );
        elephc_pdo_close(ro);
        let _ = std::fs::remove_file(&path);
    }

    /// v17 (P2-9): a `file:` URI-DSN body enables `SQLITE_OPEN_URI`, so a
    /// `mode=ro` query parameter takes effect (SQLite docs: `mode=` overrides the
    /// flags passed to `sqlite3_open_v2`) and fails to open a nonexistent
    /// database rather than silently creating a new file at the literal
    /// (unparsed) `file:...?mode=ro` path, which the pre-fix code did.
    #[test]
    fn sqlite_file_uri_dsn_mode_ro_nonexistent_fails() {
        let path = std::env::temp_dir().join(format!(
            "elephc_pdo_test_uri_ro_{}_{}.sqlite",
            std::process::id(),
            next_id()
        ));
        let _ = std::fs::remove_file(&path);
        let dsn = cs(&format!("sqlite:file:{}?mode=ro", path.display()));
        let id = unsafe {
            elephc_pdo_open_persistent(
                dsn.as_ptr(),
                0,
                0,
                std::ptr::null(),
                std::ptr::null(),
                0,
                std::ptr::null(),
                std::ptr::null(),
            )
        };
        assert_eq!(
            id, -1,
            "mode=ro URI DSN against a nonexistent file should fail to open, not create it"
        );
        assert!(!path.exists(), "a literal file must not have been created");
    }

    /// v17 (P2-16): `elephc_pdo_stmt_readonly` reports `1` for a SELECT statement
    /// and `0` for an INSERT, backing
    /// `PDOStatement::getAttribute(Pdo\Sqlite::ATTR_READONLY_STATEMENT)` as a live
    /// `sqlite3_stmt_readonly` read. An unknown handle reports `0`.
    #[test]
    fn sqlite_stmt_readonly_reports_select_vs_write() {
        let dsn = cs("sqlite::memory:");
        let conn = unsafe { elephc_pdo_open(dsn.as_ptr()) };
        assert!(conn > 0, "open failed");
        let ddl = cs("CREATE TABLE t (n INTEGER)");
        assert_eq!(unsafe { elephc_pdo_exec(conn, ddl.as_ptr()) }, 0);

        let sel = cs("SELECT n FROM t");
        let sel_stmt = unsafe { elephc_pdo_prepare(conn, sel.as_ptr(), 0) };
        assert!(sel_stmt > 0, "prepare failed");
        assert_eq!(elephc_pdo_stmt_readonly(sel_stmt), 1);

        let ins = cs("INSERT INTO t VALUES (1)");
        let ins_stmt = unsafe { elephc_pdo_prepare(conn, ins.as_ptr(), 0) };
        assert!(ins_stmt > 0, "prepare failed");
        assert_eq!(elephc_pdo_stmt_readonly(ins_stmt), 0);

        assert_eq!(elephc_pdo_stmt_readonly(999_999), 0);

        assert_eq!(elephc_pdo_finalize(sel_stmt), 1);
        assert_eq!(elephc_pdo_finalize(ins_stmt), 1);
        elephc_pdo_close(conn);
    }

    /// Verifies PHP 8.5 SQLite transaction modes are stored, validated, and used by begin.
    #[test]
    fn sqlite_transaction_mode_round_trips_and_rejects_invalid_values() {
        let dsn = cs("sqlite::memory:");
        let conn = unsafe { elephc_pdo_open(dsn.as_ptr()) };
        assert!(conn > 0, "open failed");
        assert_eq!(elephc_pdo_transaction_mode(conn), 0);
        assert_eq!(elephc_pdo_set_transaction_mode(conn, 1), 1);
        assert_eq!(elephc_pdo_transaction_mode(conn), 1);
        assert_eq!(elephc_pdo_begin(conn), 1);
        assert_eq!(elephc_pdo_in_transaction(conn), 1);
        assert_eq!(elephc_pdo_rollback(conn), 1);
        assert_eq!(elephc_pdo_set_transaction_mode(conn, 3), 0);
        assert_eq!(elephc_pdo_transaction_mode(conn), 1);
        assert_eq!(elephc_pdo_transaction_mode(999_999), -1);
        elephc_pdo_close(conn);
    }

    /// Verifies PHP 8.5 SQLite statement busy and explain attributes use live SQLite state.
    #[test]
    fn sqlite_statement_busy_and_explain_attributes_are_live() {
        let dsn = cs("sqlite::memory:");
        let conn = unsafe { elephc_pdo_open(dsn.as_ptr()) };
        assert!(conn > 0, "open failed");
        let sql = cs("SELECT 1");
        let stmt = unsafe { elephc_pdo_prepare(conn, sql.as_ptr(), 0) };
        assert!(stmt > 0, "prepare failed");
        assert_eq!(elephc_pdo_stmt_busy(stmt), 0);
        assert_eq!(elephc_pdo_stmt_explain_mode(stmt), 0);
        assert_eq!(elephc_pdo_stmt_set_explain_mode(stmt, 1), 1);
        assert_eq!(elephc_pdo_stmt_explain_mode(stmt), 1);
        assert_eq!(elephc_pdo_stmt_set_explain_mode(stmt, 3), 0);
        assert_eq!(elephc_pdo_step(stmt), 1);
        assert_eq!(elephc_pdo_stmt_busy(stmt), 1);
        assert_eq!(elephc_pdo_reset(stmt), 1);
        assert_eq!(elephc_pdo_stmt_busy(stmt), 0);
        assert_eq!(elephc_pdo_finalize(stmt), 1);
        elephc_pdo_close(conn);
    }

    /// Fixture test: the SQLite→SQLSTATE mapping mirrors php-src's
    /// `pdo_sqlite_error` table (`ext/pdo_sqlite/sqlite_driver.c`). Pinning the
    /// pairs here as data (rather than exercising them only indirectly through a
    /// live error) turns any future drift from upstream's table into a
    /// mechanical, one-line diff instead of a silent behavior change.
    #[test]
    fn sqlite_sqlstate_fixture_matches_php_src() {
        use libsqlite3_sys as ffi;
        let cases = [
            (ffi::SQLITE_OK, "00000"),
            (ffi::SQLITE_ERROR, "HY000"),
            (ffi::SQLITE_CONSTRAINT, "23000"),
            (ffi::SQLITE_BUSY, "HY000"),
            (ffi::SQLITE_LOCKED, "HY000"),
            (ffi::SQLITE_READONLY, "HY000"),
            (ffi::SQLITE_PERM, "HY000"),
            (ffi::SQLITE_NOTADB, "HY000"),
            (ffi::SQLITE_NOTFOUND, "42S02"),
            (ffi::SQLITE_INTERRUPT, "01002"),
            (ffi::SQLITE_NOLFS, "HYC00"),
            (ffi::SQLITE_TOOBIG, "22001"),
        ];
        for (code, expected) in cases {
            assert_eq!(
                sqlite::sqlite_sqlstate(code),
                expected,
                "sqlite result code {code} mapped wrong"
            );
        }
    }

    /// Placeholder translation: `?` → `$1`, `:name` → `$N` (deduped), with
    /// `'…'` literals and the `::` cast operator left untouched.
    #[test]
    fn pg_translate_placeholders() {
        let (sql, map, mixed) = pg::translate_placeholders(
            "SELECT * FROM t WHERE a = ? AND b = :b AND c = :b AND d = 'x?:y' AND e = id::text",
        );
        assert_eq!(
            sql,
            "SELECT * FROM t WHERE a = $1 AND b = $2 AND c = $2 AND d = 'x?:y' AND e = id::text"
        );
        assert_eq!(map.get("b"), Some(&2));
        assert!(mixed, "a positional ? and a named :b were both used");
    }

    /// A `--` line comment's `?` is left untouched; only the trailing real
    /// placeholder is translated.
    #[test]
    fn pg_translate_placeholders_line_comment() {
        let (sql, map, mixed) = pg::translate_placeholders("-- x = ?\nSELECT ?");
        assert_eq!(sql, "-- x = ?\nSELECT $1");
        assert!(map.is_empty());
        assert!(!mixed);
    }

    /// A `/* ... */` block comment's `?` and `:a` are left untouched; only the
    /// trailing real placeholder is translated.
    #[test]
    fn pg_translate_placeholders_block_comment() {
        let (sql, map, mixed) = pg::translate_placeholders("/* ? :a */ SELECT ?");
        assert_eq!(sql, "/* ? :a */ SELECT $1");
        assert!(map.is_empty());
        assert!(!mixed);
    }

    /// A `'...'` single-quoted literal's `?`/`::` are preserved verbatim.
    #[test]
    fn pg_translate_placeholders_single_quote() {
        let (sql, map, mixed) = pg::translate_placeholders("SELECT '?::x', ?");
        assert_eq!(sql, "SELECT '?::x', $1");
        assert!(map.is_empty());
        assert!(!mixed);
    }

    /// An unterminated PostgreSQL quote backtracks to ordinary text, so a later
    /// placeholder is still visible exactly as in php-src's re2c scanner.
    #[test]
    fn pg_translate_placeholders_unterminated_quote_backtracks() {
        let (sql, map, mixed) = pg::translate_placeholders("SELECT 'unterminated ?");
        assert_eq!(sql, "SELECT 'unterminated $1");
        assert!(map.is_empty());
        assert!(!mixed);
    }

    /// An unterminated PostgreSQL block-comment opener does not consume to EOF;
    /// the positional marker after it remains a bind slot.
    #[test]
    fn pg_translate_placeholders_unterminated_block_comment_backtracks() {
        let (sql, map, mixed) = pg::translate_placeholders("SELECT /* unterminated ?");
        assert_eq!(sql, "SELECT /* unterminated $1");
        assert!(map.is_empty());
        assert!(!mixed);
    }

    /// A `"..."` double-quoted identifier's `?` is preserved verbatim.
    #[test]
    fn pg_translate_placeholders_double_quote() {
        let (sql, map, mixed) = pg::translate_placeholders("SELECT \"we?rd\" , ?");
        assert_eq!(sql, "SELECT \"we?rd\" , $1");
        assert!(map.is_empty());
        assert!(!mixed);
    }

    /// A `$$...$$` (empty-tag) dollar-quoted string's body is preserved
    /// verbatim, including the `?` inside it.
    #[test]
    fn pg_translate_placeholders_dollar_quote_empty_tag() {
        let (sql, map, mixed) = pg::translate_placeholders("SELECT $$a ? b$$, ?");
        assert_eq!(sql, "SELECT $$a ? b$$, $1");
        assert!(map.is_empty());
        assert!(!mixed);
    }

    /// A `$tag$...$tag$` (named-tag) dollar-quoted string's body is preserved
    /// verbatim, and the trailing `:n` still translates to `$1`.
    #[test]
    fn pg_translate_placeholders_dollar_quote_tagged() {
        let (sql, map, mixed) = pg::translate_placeholders("SELECT $x$ p?q $x$ , :n");
        assert_eq!(sql, "SELECT $x$ p?q $x$ , $1");
        assert_eq!(map.get("n"), Some(&1));
        assert!(!mixed);
    }

    /// A `$` immediately followed by a digit (e.g. a literal `$1` in the input)
    /// can never open a dollar-quote tag and is emitted verbatim, distinct from
    /// the real placeholder translation.
    #[test]
    fn pg_translate_placeholders_dollar_digit_not_a_tag() {
        let (sql, map, mixed) = pg::translate_placeholders("SELECT $2foo, ?");
        assert_eq!(sql, "SELECT $2foo, $1");
        assert!(map.is_empty());
        assert!(!mixed);
    }

    /// `??` is PostgreSQL's jsonb operator escape: it collapses to a single
    /// literal `?` and allocates no placeholder slot.
    #[test]
    fn pg_translate_placeholders_jsonb_escape() {
        let (sql, map, mixed) = pg::translate_placeholders("SELECT d ?? 'k'");
        assert_eq!(sql, "SELECT d ? 'k'");
        assert!(map.is_empty());
        assert!(!mixed);
    }

    /// An `E'...'` escape-string is not terminated early by its backslash-
    /// escaped quote; the trailing `?` still translates.
    #[test]
    fn pg_translate_placeholders_e_string_backslash_escape() {
        let (sql, map, mixed) = pg::translate_placeholders("SELECT E'it\\'s', ?");
        assert_eq!(sql, "SELECT E'it\\'s', $1");
        assert!(map.is_empty());
        assert!(!mixed);
    }

    /// Repeated named placeholders dedupe to the same index.
    #[test]
    fn pg_translate_placeholders_named_dedup() {
        let (sql, map, mixed) = pg::translate_placeholders("SELECT :a, :a, :b");
        assert_eq!(sql, "SELECT $1, $1, $2");
        assert_eq!(map.get("a"), Some(&1));
        assert_eq!(map.get("b"), Some(&2));
        assert!(!mixed);
    }

    /// The `::` cast operator is left untouched (not read as a named
    /// placeholder), while a real `?` still translates.
    #[test]
    fn pg_translate_placeholders_cast_operator() {
        let (sql, map, mixed) = pg::translate_placeholders("SELECT x::int, ?");
        assert_eq!(sql, "SELECT x::int, $1");
        assert!(map.is_empty());
        assert!(!mixed);
    }

    /// A SQL text using both a positional `?` and a named `:a` sets the
    /// `mixed` flag, which `PgConn::prepare()` uses to reject the statement
    /// with `HY093` before ever asking the server to prepare it.
    #[test]
    fn pg_translate_placeholders_mixed_flag() {
        let (_, _, mixed) = pg::translate_placeholders("SELECT ?, :a");
        assert!(mixed);
    }

    /// BUG 1 regression: multi-byte UTF-8 bytes inside a `'...'` string literal
    /// must round-trip byte-for-byte. A per-byte `u8 as char` cast would
    /// double-encode any byte >= 0x80 (a UTF-8 continuation byte reinterpreted
    /// as a Latin-1 codepoint), corrupting `café`/`Zürich` before the SQL ever
    /// reaches the server.
    #[test]
    fn pg_translate_placeholders_utf8_string_literal_preserved() {
        let (sql, map, mixed) = pg::translate_placeholders("SELECT 'café', ? , 'Zürich'");
        assert_eq!(sql, "SELECT 'café', $1 , 'Zürich'");
        assert!(map.is_empty());
        assert!(!mixed);
    }

    /// BUG 1 regression: a multi-byte UTF-8 byte outside any recognized quoted
    /// region (the ordinary/unquoted scanning path) must also round-trip
    /// unmangled.
    #[test]
    fn pg_translate_placeholders_utf8_outside_quotes_preserved() {
        let (sql, map, mixed) = pg::translate_placeholders("SELECT résumé, ?");
        assert_eq!(sql, "SELECT résumé, $1");
        assert!(map.is_empty());
        assert!(!mixed);
    }

    /// BUG 2 regression: a `:name`-shaped token immediately preceded by an
    /// alphanumeric byte is not a bind placeholder (matching php-src's
    /// `pdo_sql_parser.re`) — most importantly a PostgreSQL array slice like
    /// `data[1:5]`, which must not be misread as a named parameter `:5`.
    #[test]
    fn pg_translate_placeholders_array_slice_not_named_param() {
        let (sql, map, mixed) =
            pg::translate_placeholders("SELECT data[1:5] FROM t WHERE id = ?");
        assert_eq!(sql, "SELECT data[1:5] FROM t WHERE id = $1");
        assert!(map.is_empty());
        assert!(!mixed);
    }

    /// F-PARSE-01 (audit corpus #10), PostgreSQL half — the exact mirror of
    /// `my_translate_placeholders_triple_colon_not_named`. php-src's pgsql scanner
    /// rule is `MULTICHAR = [:]{2,}` (`pgsql_sql_parser.re:35`), and re2c matches by
    /// maximal munch: an ODD run of colons is ONE verbatim text token, not a `::`
    /// cast pair followed by a fresh named placeholder. Consuming colons two at a
    /// time left the third colon of `:::c` to be re-scanned as `:c`, conjuring a
    /// phantom named bind real PHP never emits — and with it a bind-count
    /// disagreement between elephc and the server. Only the trailing `?` is a slot,
    /// so the map stays empty and the statement is not "mixed".
    #[test]
    fn pg_translate_placeholders_triple_colon_not_named() {
        let (sql, map, mixed) = pg::translate_placeholders("SELECT a WHERE b :::c AND d = ?");
        assert_eq!(sql, "SELECT a WHERE b :::c AND d = $1");
        assert!(map.is_empty(), "`:::c` must not allocate a named bind slot");
        assert!(!mixed);
    }

    /// The even-length counterpart of the greedy-run rule above (pg side). A 4-colon
    /// run was already emitted verbatim by the old pairwise loop (two exact pairs,
    /// nothing left over), so this pins that rewriting the `:` arm to consume the
    /// WHOLE run did not regress the case that used to work: `::::c` still yields no
    /// named bind and the lone `?` stays the only slot. Together with
    /// `pg_translate_placeholders_cast_operator` (the 2-colon case) the three
    /// lengths 2/3/4 are covered.
    #[test]
    fn pg_translate_placeholders_quadruple_colon_not_named() {
        let (sql, map, mixed) = pg::translate_placeholders("SELECT a WHERE b ::::c AND d = ?");
        assert_eq!(sql, "SELECT a WHERE b ::::c AND d = $1");
        assert!(map.is_empty(), "`::::c` must not allocate a named bind slot");
        assert!(!mixed);
    }

    /// F-PARSE-02 (audit corpus #12): a dollar-quote TAG may contain non-ASCII
    /// bytes. php-src's pgsql scanner spells the classes `DOLQ_START =
    /// [A-Za-z\200-\377_]` and `DOLQ_CONT = [A-Za-z\200-\377_0-9]`
    /// (`pgsql_sql_parser.re:32-33`), matching PostgreSQL's own lexer, so
    /// `$café$ … $café$` is a real dollar-quoted string. Gating the tag on
    /// `is_ascii_alphabetic()` meant the quote never opened, the body fell through
    /// to the ordinary scanner, and the `?` INSIDE the string literal was rewritten
    /// into a bind — corrupting the SQL text and inventing a parameter. The ASCII
    /// contrast (`$cafe$`) is asserted alongside so the test shows the delta is the
    /// non-ASCII tag byte and nothing else: both must preserve the body verbatim and
    /// translate only the trailing `?` to `$1`.
    #[test]
    fn pg_translate_placeholders_non_ascii_dollar_quote_tag() {
        let (sql, map, mixed) = pg::translate_placeholders("SELECT $café$ a ? b $café$, ?");
        assert_eq!(sql, "SELECT $café$ a ? b $café$, $1");
        assert!(
            map.is_empty(),
            "the `?` inside a `$café$` literal must not become a bind"
        );
        assert!(!mixed);

        let (ascii_sql, ascii_map, ascii_mixed) =
            pg::translate_placeholders("SELECT $cafe$ a ? b $cafe$, ?");
        assert_eq!(ascii_sql, "SELECT $cafe$ a ? b $cafe$, $1");
        assert!(ascii_map.is_empty());
        assert!(!ascii_mixed);
    }

    /// A `pgsql:` DSN parses into a libpq connection string.
    #[test]
    fn pg_dsn_parses() {
        let s = pg::parse_dsn("pgsql:host=localhost;port=5432;dbname=app").unwrap();
        assert!(s.contains("host='localhost'"), "got: {s}");
        assert!(s.contains("dbname='app'"), "got: {s}");
    }

    /// F-PG-03 / F-CORE-10: php-src's pgsql handle factory bounds EVERY connect —
    /// `pgsql_driver.c:1350,1373,1381` default `connect_timeout` to 30 s and always
    /// append it to the conninfo — so a black-holed host fails in seconds rather
    /// than hanging. elephc forwarded no timeout at all, and the pure-Rust
    /// `postgres` client has no application-level connect bound of its own, so the
    /// connect could hang for minutes. The default is now folded into the conninfo
    /// whenever the caller supplied none (the prelude folds `PDO::ATTR_TIMEOUT` into
    /// the DSN under this very same key, so both seams land on this one check).
    #[test]
    fn pg_dsn_defaults_connect_timeout_to_30s() {
        let s = pg::parse_dsn("pgsql:host=localhost;dbname=app").unwrap();
        assert!(
            s.contains("connect_timeout='30'"),
            "an unbounded connect must be bounded at php-src's 30 s; got: {s}"
        );
    }

    /// The other half of F-PG-03: a `connect_timeout` the caller spelled out (in the
    /// DSN body, or via `PDO::ATTR_TIMEOUT`, which the prelude folds into the DSN
    /// under the same key) WINS — the 30 s default only fills a gap. This is a
    /// deliberate, documented divergence from php-src, which overwrites the
    /// DSN-supplied value with its own; silently ignoring an explicit timeout would
    /// be the more surprising behavior.
    #[test]
    fn pg_dsn_explicit_connect_timeout_wins_over_the_default() {
        let s = pg::parse_dsn("pgsql:host=localhost;dbname=app;connect_timeout=5").unwrap();
        assert!(s.contains("connect_timeout='5'"), "got: {s}");
        assert!(
            !s.contains("connect_timeout='30'"),
            "the default must not be appended on top of an explicit value; got: {s}"
        );
    }

    /// Libpq resolves a bare `pgsql:` against the operating-system username rather
    /// than rejecting it before connection. The native resolver mirrors that and
    /// still applies php-src's default connect timeout.
    #[test]
    fn pg_dsn_empty_body_uses_libpq_os_user_default() {
        let connection = pg::parse_dsn("pgsql:").expect("OS user supplies libpq default");
        assert!(connection.contains("user='"));
        assert!(connection.contains("connect_timeout='30'"));
    }

    /// Full PostgreSQL round-trip against a live server. Ignored by default; run
    /// with `ELEPHC_PG_TEST_DSN` — or, since F-QUAL-06, the codegen suite's own
    /// `ELEPHC_PG_DSN` — set, e.g.
    /// `ELEPHC_PG_DSN='pgsql:host=localhost;port=55432;dbname=testdb;user=test;password=test'`.
    /// Also covers the v7 additions: `elephc_pdo_bind_bool`, `elephc_pdo_bind_blob`
    /// (an embedded-NUL blob and a null-pointer→NULL bind), `elephc_pdo_sqlstate`/
    /// `elephc_pdo_stmt_sqlstate` (`"00000"` after a success, a real SQLSTATE after
    /// a forced duplicate-key error, and a reset back to `"00000"` after the next
    /// successful `prepare()`), and `elephc_pdo_server_version`.
    #[test]
    #[ignore]
    fn pg_round_trip() {
        let Some(dsn) = live_dsn("ELEPHC_PG_TEST_DSN", "ELEPHC_PG_DSN") else {
            return;
        };
        let dsn = cs(&dsn);
        let conn = unsafe { elephc_pdo_open(dsn.as_ptr()) };
        assert!(
            conn > 0,
            "pg open failed: {}",
            unsafe { read(elephc_pdo_last_open_error()) }
        );

        let version = unsafe { read(elephc_pdo_server_version(conn)) };
        assert!(!version.is_empty(), "server_version was empty");

        let drop = cs("DROP TABLE IF EXISTS pdo_rt");
        unsafe { elephc_pdo_exec(conn, drop.as_ptr()) };
        let ddl = cs(
            "CREATE TABLE pdo_rt (id SERIAL PRIMARY KEY, name TEXT, score DOUBLE PRECISION, flag BOOLEAN, data BYTEA)",
        );
        unsafe { elephc_pdo_exec(conn, ddl.as_ptr()) };
        assert_eq!(unsafe { read(elephc_pdo_sqlstate(conn)) }, "00000");

        let ins = cs("INSERT INTO pdo_rt (name, score, flag, data) VALUES (:n, :s, :f, :d)");
        let stmt = unsafe { elephc_pdo_prepare(conn, ins.as_ptr(), 0) };
        assert!(stmt > 0, "pg prepare failed");
        let n = cs(":n");
        let ni = unsafe { elephc_pdo_bind_parameter_index(stmt, n.as_ptr()) };
        let s = cs(":s");
        let si = unsafe { elephc_pdo_bind_parameter_index(stmt, s.as_ptr()) };
        let f = cs(":f");
        let fi = unsafe { elephc_pdo_bind_parameter_index(stmt, f.as_ptr()) };
        let d = cs(":d");
        let di = unsafe { elephc_pdo_bind_parameter_index(stmt, d.as_ptr()) };
        let ada = cs("Ada");
        unsafe { elephc_pdo_bind_text(stmt, ni, ada.as_ptr(), ada.as_bytes().len() as i64) };
        elephc_pdo_bind_double(stmt, si, 9.5);
        elephc_pdo_bind_bool(stmt, fi, 1);
        let blob = b"A\0B";
        unsafe {
            elephc_pdo_bind_blob(stmt, di, blob.as_ptr() as *const c_char, blob.len() as i64)
        };
        assert_eq!(elephc_pdo_step(stmt), 0);
        elephc_pdo_finalize(stmt);
        assert_eq!(unsafe { read(elephc_pdo_sqlstate(conn)) }, "00000");

        let lid = unsafe { elephc_pdo_last_insert_id(conn, std::ptr::null()) };
        assert_eq!(lid, 1);

        // Bug 1 regression coverage: a null-pointer blob bind stores SQL NULL
        // rather than an empty blob.
        let ins2 = cs("INSERT INTO pdo_rt (name, score, flag, data) VALUES (:n, :s, :f, :d)");
        let stmt2 = unsafe { elephc_pdo_prepare(conn, ins2.as_ptr(), 0) };
        assert!(stmt2 > 0, "pg prepare failed");
        let ni2 = unsafe { elephc_pdo_bind_parameter_index(stmt2, n.as_ptr()) };
        let si2 = unsafe { elephc_pdo_bind_parameter_index(stmt2, s.as_ptr()) };
        let fi2 = unsafe { elephc_pdo_bind_parameter_index(stmt2, f.as_ptr()) };
        let di2 = unsafe { elephc_pdo_bind_parameter_index(stmt2, d.as_ptr()) };
        let grace = cs("Grace");
        unsafe { elephc_pdo_bind_text(stmt2, ni2, grace.as_ptr(), grace.as_bytes().len() as i64) };
        elephc_pdo_bind_double(stmt2, si2, 1.0);
        elephc_pdo_bind_bool(stmt2, fi2, 0);
        unsafe { elephc_pdo_bind_blob(stmt2, di2, std::ptr::null(), 0) };
        assert_eq!(elephc_pdo_step(stmt2), 0);
        elephc_pdo_finalize(stmt2);

        let sel = cs("SELECT id, name, score, flag, data FROM pdo_rt WHERE id = ?");
        let q = unsafe { elephc_pdo_prepare(conn, sel.as_ptr(), 0) };
        elephc_pdo_bind_int(q, 1, 1);
        assert_eq!(elephc_pdo_step(q), 1);
        assert_eq!(elephc_pdo_column_int(q, 0), 1);
        assert_eq!(unsafe { read(elephc_pdo_column_name(q, 1)) }, "name");
        // v24/F-QUAL-03: the NUL-stripping `elephc_pdo_column_text` is gone; a text
        // column is read through the same byte-exact len+ptr pair the prelude uses.
        let name_len = elephc_pdo_column_data_len(q, 1);
        let name_ptr = elephc_pdo_column_data_ptr(q, 1);
        assert_eq!(unsafe { read_bytes(name_ptr, name_len) }, b"Ada");
        assert_eq!(elephc_pdo_column_double(q, 2), 9.5);
        assert_eq!(elephc_pdo_column_int(q, 3), 1);
        assert_eq!(elephc_pdo_column_data_len(q, 4), 3);
        let ptr = elephc_pdo_column_data_ptr(q, 4);
        assert_eq!(unsafe { read_bytes(ptr, 3) }, b"A\0B");
        assert_eq!(elephc_pdo_step(q), 0);
        elephc_pdo_finalize(q);

        let sel2 = cs("SELECT data FROM pdo_rt WHERE id = 2");
        let q2 = unsafe { elephc_pdo_prepare(conn, sel2.as_ptr(), 0) };
        assert!(q2 > 0, "pg prepare failed");
        assert_eq!(elephc_pdo_step(q2), 1);
        assert_eq!(elephc_pdo_column_type(q2, 0), 5, "null-pointer blob bind must read back as NULL");
        elephc_pdo_finalize(q2);

        // Bug 2 regression coverage: a forced duplicate-key error reports a
        // non-"00000" SQLSTATE at both the connection and statement level, and
        // the following successful prepare() resets it back to "00000".
        let dup = cs("INSERT INTO pdo_rt (id, name) VALUES (1, 'dup')");
        let dup_stmt = unsafe { elephc_pdo_prepare(conn, dup.as_ptr(), 0) };
        assert!(dup_stmt > 0, "pg prepare failed");
        assert_eq!(elephc_pdo_step(dup_stmt), -1);
        let dup_state = unsafe { read(elephc_pdo_sqlstate(conn)) };
        assert_ne!(dup_state, "00000", "expected a real SQLSTATE, got: {dup_state}");
        assert_eq!(unsafe { read(elephc_pdo_stmt_sqlstate(dup_stmt)) }, dup_state);
        elephc_pdo_finalize(dup_stmt);

        let sel3 = cs("SELECT 1");
        let ok_stmt = unsafe { elephc_pdo_prepare(conn, sel3.as_ptr(), 0) };
        assert!(ok_stmt > 0, "pg prepare failed");
        assert_eq!(unsafe { read(elephc_pdo_sqlstate(conn)) }, "00000");
        elephc_pdo_finalize(ok_stmt);

        let cleanup = cs("DROP TABLE pdo_rt");
        unsafe { elephc_pdo_exec(conn, cleanup.as_ptr()) };
        elephc_pdo_close(conn);
    }

    /// MySQL placeholder translation: `?` and `:name` both become `?`, with the
    /// per-`?` `order` reusing one slot for a repeated name and `'…'` literals and
    /// `::` left untouched.
    #[test]
    fn my_translate_placeholders() {
        let (sql, map, order, mixed) = my::translate_placeholders(
            "SELECT * FROM t WHERE a = ? AND b = :b AND c = :b AND d = 'x?:y' AND e = id::text",
            false,
        );
        assert_eq!(
            sql,
            "SELECT * FROM t WHERE a = ? AND b = ? AND c = ? AND d = 'x?:y' AND e = id::text"
        );
        // `?`→slot 1, `:b`→slot 2 (reused for the second `:b`).
        assert_eq!(order, vec![1, 2, 2]);
        assert_eq!(map.get("b"), Some(&2));
        assert!(mixed, "a positional ? and a named :b were both used");
    }

    /// A `--` line comment's `?` is left untouched; only the trailing real
    /// placeholder is translated.
    #[test]
    fn my_translate_placeholders_line_comment() {
        let (sql, map, order, mixed) = my::translate_placeholders("-- ?\nSELECT ?", false);
        assert_eq!(sql, "-- ?\nSELECT ?");
        assert_eq!(order, vec![1]);
        assert!(map.is_empty());
        assert!(!mixed);
    }

    /// `--` NOT followed by whitespace is not a MySQL comment (`a--b` is the
    /// arithmetic `a - -b`), so a `?` after it is a real placeholder — matching
    /// php-src's `mysql_sql_parser.re` COMMENTS rule (`"--"[ \t\v\f\r]`).
    #[test]
    fn my_translate_placeholders_double_dash_not_comment() {
        let (sql, map, order, mixed) = my::translate_placeholders("SELECT a--b, ? FROM t", false);
        assert_eq!(sql, "SELECT a--b, ? FROM t");
        assert_eq!(order, vec![1]);
        assert!(map.is_empty());
        assert!(!mixed);
    }

    /// A bare `--` (no trailing whitespace) does not open a comment, so both the
    /// positional `?` and the named `:c` after it are real placeholders — which
    /// makes the statement mixed (HY093 at prepare).
    #[test]
    fn my_translate_placeholders_double_dash_keeps_mixed() {
        let (_sql, map, order, mixed) = my::translate_placeholders("SELECT ?--:c", false);
        assert_eq!(order, vec![1, 2]);
        assert_eq!(map.get("c"), Some(&2));
        assert!(mixed);
    }

    /// A `#` line comment's `?` is left untouched; only the trailing real
    /// placeholder is translated.
    #[test]
    fn my_translate_placeholders_hash_comment() {
        let (sql, map, order, mixed) = my::translate_placeholders("# ?\nSELECT ?", false);
        assert_eq!(sql, "# ?\nSELECT ?");
        assert_eq!(order, vec![1]);
        assert!(map.is_empty());
        assert!(!mixed);
    }

    /// A `/* ... */` block comment's `?` is left untouched; only the trailing
    /// real placeholder is translated.
    #[test]
    fn my_translate_placeholders_block_comment() {
        let (sql, map, order, mixed) = my::translate_placeholders("/* ? */ SELECT ?", false);
        assert_eq!(sql, "/* ? */ SELECT ?");
        assert_eq!(order, vec![1]);
        assert!(map.is_empty());
        assert!(!mixed);
    }

    /// A `"..."` double-quoted string literal's `?` is preserved verbatim (both
    /// quote styles are string literals in MySQL's default `sql_mode`).
    #[test]
    fn my_translate_placeholders_double_quote_string() {
        let (sql, map, order, mixed) = my::translate_placeholders("SELECT \"a?b\", ?", false);
        assert_eq!(sql, "SELECT \"a?b\", ?");
        assert_eq!(order, vec![1]);
        assert!(map.is_empty());
        assert!(!mixed);
    }

    /// An unterminated MySQL quote falls back to ordinary text, leaving the
    /// following question mark visible as a positional bind.
    #[test]
    fn my_translate_placeholders_unterminated_quote_backtracks() {
        let (sql, map, order, mixed) =
            my::translate_placeholders("SELECT 'unterminated ?", false);
        assert_eq!(sql, "SELECT 'unterminated ?");
        assert_eq!(order, vec![1]);
        assert!(map.is_empty());
        assert!(!mixed);
    }

    /// An unterminated MySQL block comment does not hide a later positional
    /// marker from PDO's placeholder scanner.
    #[test]
    fn my_translate_placeholders_unterminated_block_comment_backtracks() {
        let (sql, map, order, mixed) =
            my::translate_placeholders("SELECT /* unterminated ?", false);
        assert_eq!(sql, "SELECT /* unterminated ?");
        assert_eq!(order, vec![1]);
        assert!(map.is_empty());
        assert!(!mixed);
    }

    /// A backslash-escaped quote inside a `'...'` literal does not terminate
    /// the string early.
    #[test]
    fn my_translate_placeholders_backslash_in_single_quote() {
        let (sql, map, order, mixed) = my::translate_placeholders("SELECT 'a\\'b', ?", false);
        assert_eq!(sql, "SELECT 'a\\'b', ?");
        assert_eq!(order, vec![1]);
        assert!(map.is_empty());
        assert!(!mixed);
    }

    /// A backtick-quoted identifier's `?` is preserved verbatim.
    #[test]
    fn my_translate_placeholders_backtick_identifier() {
        let (sql, map, order, mixed) = my::translate_placeholders("SELECT `we?rd`, ?", false);
        assert_eq!(sql, "SELECT `we?rd`, ?");
        assert_eq!(order, vec![1]);
        assert!(map.is_empty());
        assert!(!mixed);
    }

    /// A run of two `?` is not two positional placeholders (P1-a): MySQL has no
    /// `?` operators to unescape the way PostgreSQL's jsonb `?`/`?|`/`?&` do, so
    /// the run is emitted verbatim as a text token with no slot allocated —
    /// `order` stays `[1]`, counting only the lone trailing `?`. The mirror of
    /// the PostgreSQL `pg_translate_placeholders_jsonb_escape` coverage; the
    /// load-bearing property both pin is "`??` allocates no bind slot".
    #[test]
    fn my_translate_placeholders_double_question_no_slot() {
        let (sql, map, order, mixed) = my::translate_placeholders("SELECT a ?? b, ?", false);
        assert_eq!(sql, "SELECT a ?? b, ?");
        assert_eq!(order, vec![1]);
        assert!(map.is_empty());
        assert!(!mixed);
    }

    /// A named placeholder reused twice shares one slot in both `order` and
    /// the name map.
    #[test]
    fn my_translate_placeholders_named_reuse() {
        let (sql, map, order, mixed) = my::translate_placeholders("SELECT :a, :a", false);
        assert_eq!(sql, "SELECT ?, ?");
        assert_eq!(order, vec![1, 1]);
        assert_eq!(map.get("a"), Some(&1));
        assert!(!mixed);
    }

    /// A SQL text using both a positional `?` and a named `:a` sets the
    /// `mixed` flag, which `MyConn::prepare()` uses to reject the statement
    /// with `HY093` before ever asking the server to prepare it.
    #[test]
    fn my_translate_placeholders_mixed_flag() {
        let (_, _, _, mixed) = my::translate_placeholders("SELECT ?, :a", false);
        assert!(mixed);
    }

    /// BUG 1 regression: multi-byte UTF-8 bytes inside a `'...'` string
    /// literal must round-trip byte-for-byte. A per-byte `u8 as char` cast
    /// would double-encode any byte >= 0x80, corrupting `café` before the SQL
    /// ever reaches the server.
    #[test]
    fn my_translate_placeholders_utf8_string_literal_preserved() {
        let (sql, map, order, mixed) = my::translate_placeholders("SELECT 'café', ?", false);
        assert_eq!(sql, "SELECT 'café', ?");
        assert_eq!(order, vec![1]);
        assert!(map.is_empty());
        assert!(!mixed);
    }

    /// BUG 2 regression: a `:name`-shaped token immediately preceded by an
    /// alphanumeric byte is not a bind placeholder (matching php-src's
    /// `pdo_sql_parser.re`), so it is left untouched and allocates no slot —
    /// only the real trailing `?` does.
    #[test]
    fn my_translate_placeholders_colon_after_alnum_not_named() {
        let (sql, map, order, mixed) = my::translate_placeholders("SELECT a:b, ?", false);
        assert_eq!(sql, "SELECT a:b, ?");
        assert_eq!(order, vec![1]);
        assert!(map.is_empty());
        assert!(!mixed);
    }

    /// F-PARSE-01 (audit corpus #10): an ODD run of colons is one verbatim token,
    /// not a `::` pair followed by a fresh named placeholder. php-src's
    /// `MULTICHAR = [:]{2,}` rule is re2c-greedy — maximal munch swallows the whole
    /// contiguous run — so `:::c` emits no bind at all. Consuming colons two at a
    /// time left the third colon of the run to be re-scanned as `:c`, conjuring a
    /// phantom named bind real PHP never emits (and, with it, a bind-count
    /// disagreement between elephc and the server). Only the trailing `?` is a slot.
    #[test]
    fn my_translate_placeholders_triple_colon_not_named() {
        let (sql, map, order, mixed) =
            my::translate_placeholders("SELECT a WHERE b :::c AND d = ?", false);
        assert_eq!(sql, "SELECT a WHERE b :::c AND d = ?");
        assert_eq!(order, vec![1]);
        assert!(map.is_empty(), "`:::c` must not allocate a named bind slot");
        assert!(!mixed);
    }

    /// The even-length counterpart of the greedy-run rule above. A 4-colon run was
    /// already emitted verbatim by the old pairwise loop (two exact pairs, nothing
    /// left over), so this pins that rewriting the `:` arm to consume the WHOLE run
    /// did not regress the case that used to work — `::::c` still yields no named
    /// bind, and the lone `?` stays the only slot.
    #[test]
    fn my_translate_placeholders_quadruple_colon_not_named() {
        let (sql, map, order, mixed) =
            my::translate_placeholders("SELECT a WHERE b ::::c AND d = ?", false);
        assert_eq!(sql, "SELECT a WHERE b ::::c AND d = ?");
        assert_eq!(order, vec![1]);
        assert!(map.is_empty(), "`::::c` must not allocate a named bind slot");
        assert!(!mixed);
    }

    /// F-PARSE-07 precondition: both driver scanners must flag the SAME statement —
    /// one that mixes a positional `?` with a named `:name` — as `mixed`, since that
    /// single flag is what `PgConn::prepare()` and `MyConn::prepare()` each turn into
    /// the identical HY093 rejection php-src raises for a mixed-parameter statement.
    /// The finding itself (the two drivers reported DIFFERENT native codes in
    /// `errorInfo()[1]` for that one logical error: my = 0, pg = 1) cannot be
    /// asserted here: the native code is a field on a live `MyConn`/`PgConn`, which
    /// only a connected server can produce. What is testable without a server — that
    /// the two scanners agree the statement is rejectable at all — is pinned here.
    #[test]
    fn mixed_placeholders_flagged_by_both_scanners() {
        let sql = "SELECT * FROM t WHERE a = ? AND b = :b";
        let (_, _, pg_mixed) = pg::translate_placeholders(sql);
        let (_, _, _, my_mixed) = my::translate_placeholders(sql, false);
        assert!(pg_mixed, "the pg scanner must flag the mixed statement");
        assert!(my_mixed, "the my scanner must flag the mixed statement");
    }

    /// Full MySQL/MariaDB round-trip against a live server. Ignored by default; run
    /// with `ELEPHC_MY_TEST_DSN` — or, since F-QUAL-06, the codegen suite's own
    /// `ELEPHC_MY_DSN` — set, e.g.
    /// `ELEPHC_MY_DSN='mysql:host=localhost;port=33060;dbname=testdb;user=test;password=test'`.
    /// Also covers the v7 additions: `elephc_pdo_bind_bool`, `elephc_pdo_bind_blob`
    /// (an embedded-NUL blob and a null-pointer→NULL bind), `elephc_pdo_sqlstate`/
    /// `elephc_pdo_stmt_sqlstate` (`"00000"` after a success, a real SQLSTATE after
    /// a forced duplicate-key error, and a reset back to `"00000"` after the next
    /// successful `prepare()`), and `elephc_pdo_server_version`.
    #[test]
    #[ignore]
    fn my_round_trip() {
        let Some(dsn) = live_dsn("ELEPHC_MY_TEST_DSN", "ELEPHC_MY_DSN") else {
            return;
        };
        let dsn = cs(&dsn);
        let conn = unsafe { elephc_pdo_open(dsn.as_ptr()) };
        assert!(
            conn > 0,
            "mysql open failed: {}",
            unsafe { read(elephc_pdo_last_open_error()) }
        );
        assert_eq!(unsafe { read(elephc_pdo_driver_name(conn)) }, "mysql");

        let version = unsafe { read(elephc_pdo_server_version(conn)) };
        assert!(!version.is_empty(), "server_version was empty");

        let drop = cs("DROP TABLE IF EXISTS pdo_rt");
        unsafe { elephc_pdo_exec(conn, drop.as_ptr()) };
        let ddl = cs(
            "CREATE TABLE pdo_rt (id INTEGER PRIMARY KEY AUTO_INCREMENT, name TEXT, score DOUBLE, flag TINYINT(1), data BLOB)",
        );
        unsafe { elephc_pdo_exec(conn, ddl.as_ptr()) };
        assert_eq!(unsafe { read(elephc_pdo_sqlstate(conn)) }, "00000");

        let ins = cs("INSERT INTO pdo_rt (name, score, flag, data) VALUES (:n, :s, :f, :d)");
        let stmt = unsafe { elephc_pdo_prepare(conn, ins.as_ptr(), 0) };
        assert!(stmt > 0, "mysql prepare failed");
        let n = cs(":n");
        let ni = unsafe { elephc_pdo_bind_parameter_index(stmt, n.as_ptr()) };
        let s = cs(":s");
        let si = unsafe { elephc_pdo_bind_parameter_index(stmt, s.as_ptr()) };
        let f = cs(":f");
        let fi = unsafe { elephc_pdo_bind_parameter_index(stmt, f.as_ptr()) };
        let d = cs(":d");
        let di = unsafe { elephc_pdo_bind_parameter_index(stmt, d.as_ptr()) };
        let ada = cs("Ada");
        unsafe { elephc_pdo_bind_text(stmt, ni, ada.as_ptr(), ada.as_bytes().len() as i64) };
        elephc_pdo_bind_double(stmt, si, 9.5);
        elephc_pdo_bind_bool(stmt, fi, 1);
        let blob = b"A\0B";
        unsafe {
            elephc_pdo_bind_blob(stmt, di, blob.as_ptr() as *const c_char, blob.len() as i64)
        };
        assert_eq!(elephc_pdo_step(stmt), 0);
        elephc_pdo_finalize(stmt);
        assert_eq!(unsafe { read(elephc_pdo_sqlstate(conn)) }, "00000");

        let lid = unsafe { elephc_pdo_last_insert_id(conn, std::ptr::null()) };
        assert_eq!(lid, 1);

        // Bug 1 regression coverage: a null-pointer blob bind stores SQL NULL
        // rather than an empty blob.
        let ins2 = cs("INSERT INTO pdo_rt (name, score, flag, data) VALUES (:n, :s, :f, :d)");
        let stmt2 = unsafe { elephc_pdo_prepare(conn, ins2.as_ptr(), 0) };
        assert!(stmt2 > 0, "mysql prepare failed");
        let ni2 = unsafe { elephc_pdo_bind_parameter_index(stmt2, n.as_ptr()) };
        let si2 = unsafe { elephc_pdo_bind_parameter_index(stmt2, s.as_ptr()) };
        let fi2 = unsafe { elephc_pdo_bind_parameter_index(stmt2, f.as_ptr()) };
        let di2 = unsafe { elephc_pdo_bind_parameter_index(stmt2, d.as_ptr()) };
        let grace = cs("Grace");
        unsafe { elephc_pdo_bind_text(stmt2, ni2, grace.as_ptr(), grace.as_bytes().len() as i64) };
        elephc_pdo_bind_double(stmt2, si2, 1.0);
        elephc_pdo_bind_bool(stmt2, fi2, 0);
        unsafe { elephc_pdo_bind_blob(stmt2, di2, std::ptr::null(), 0) };
        assert_eq!(elephc_pdo_step(stmt2), 0);
        elephc_pdo_finalize(stmt2);

        let sel = cs("SELECT id, name, score, flag, data FROM pdo_rt WHERE id = ?");
        let q = unsafe { elephc_pdo_prepare(conn, sel.as_ptr(), 0) };
        elephc_pdo_bind_int(q, 1, 1);
        assert_eq!(elephc_pdo_step(q), 1);
        assert_eq!(elephc_pdo_column_int(q, 0), 1);
        assert_eq!(unsafe { read(elephc_pdo_column_name(q, 1)) }, "name");
        // v24/F-QUAL-03: the NUL-stripping `elephc_pdo_column_text` is gone; a text
        // column is read through the same byte-exact len+ptr pair the prelude uses.
        let name_len = elephc_pdo_column_data_len(q, 1);
        let name_ptr = elephc_pdo_column_data_ptr(q, 1);
        assert_eq!(unsafe { read_bytes(name_ptr, name_len) }, b"Ada");
        assert_eq!(elephc_pdo_column_double(q, 2), 9.5);
        assert_eq!(elephc_pdo_column_int(q, 3), 1);
        assert_eq!(elephc_pdo_column_data_len(q, 4), 3);
        let ptr = elephc_pdo_column_data_ptr(q, 4);
        assert_eq!(unsafe { read_bytes(ptr, 3) }, b"A\0B");
        assert_eq!(elephc_pdo_step(q), 0);
        elephc_pdo_finalize(q);

        let sel2 = cs("SELECT data FROM pdo_rt WHERE id = 2");
        let q2 = unsafe { elephc_pdo_prepare(conn, sel2.as_ptr(), 0) };
        assert!(q2 > 0, "mysql prepare failed");
        assert_eq!(elephc_pdo_step(q2), 1);
        assert_eq!(elephc_pdo_column_type(q2, 0), 5, "null-pointer blob bind must read back as NULL");
        elephc_pdo_finalize(q2);

        // Bug 2 regression coverage: a forced duplicate-key error reports a
        // non-"00000" SQLSTATE at both the connection and statement level, and
        // the following successful prepare() resets it back to "00000".
        let dup = cs("INSERT INTO pdo_rt (id, name) VALUES (1, 'dup')");
        let dup_stmt = unsafe { elephc_pdo_prepare(conn, dup.as_ptr(), 0) };
        assert!(dup_stmt > 0, "mysql prepare failed");
        assert_eq!(elephc_pdo_step(dup_stmt), -1);
        let dup_state = unsafe { read(elephc_pdo_sqlstate(conn)) };
        assert_ne!(dup_state, "00000", "expected a real SQLSTATE, got: {dup_state}");
        assert_eq!(unsafe { read(elephc_pdo_stmt_sqlstate(dup_stmt)) }, dup_state);
        elephc_pdo_finalize(dup_stmt);

        let sel3 = cs("SELECT 1");
        let ok_stmt = unsafe { elephc_pdo_prepare(conn, sel3.as_ptr(), 0) };
        assert!(ok_stmt > 0, "mysql prepare failed");
        assert_eq!(unsafe { read(elephc_pdo_sqlstate(conn)) }, "00000");
        elephc_pdo_finalize(ok_stmt);

        let cleanup = cs("DROP TABLE pdo_rt");
        unsafe { elephc_pdo_exec(conn, cleanup.as_ptr()) };
        elephc_pdo_close(conn);
    }
}
