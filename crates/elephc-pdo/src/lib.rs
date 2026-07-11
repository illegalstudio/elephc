//! Purpose:
//! Multi-driver database bridge for the elephc PDO implementation. Exposes a
//! small, stable, driver-agnostic C ABI (`elephc_pdo_*`) that the elephc PDO
//! prelude calls through `extern "elephc_pdo"` declarations; each call dispatches
//! to the SQLite, PostgreSQL, or MySQL/MariaDB driver based on the handle's
//! driver, selected from the DSN prefix at `open()`.
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
//! - The drivers are bundled (SQLite) / pure-Rust (PostgreSQL, MySQL/MariaDB), so
//!   compiled PHP binaries have no system database-client runtime dependency.

mod my;
mod pg;
mod sqlite;

use std::collections::{HashMap, HashSet};
use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_void};
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Mutex, OnceLock};

/// A live connection, tagged by its driver.
enum Conn {
    Sqlite(sqlite::SqliteConn),
    Postgres(pg::PgConn),
    Mysql(my::MyConn),
}

/// A live prepared statement, tagged by its driver.
enum Stmt {
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

/// Process-local persistent connection pool, keyed by the fully materialized
/// DSN passed into the bridge after constructor credentials have been folded in.
fn persistent_conns() -> &'static Mutex<HashMap<String, i64>> {
    static PERSISTENT_CONNS: OnceLock<Mutex<HashMap<String, i64>>> = OnceLock::new();
    PERSISTENT_CONNS.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Set of connection handles owned by the persistent pool. `elephc_pdo_close`
/// leaves these handles open so later persistent opens can reuse them.
fn persistent_ids() -> &'static Mutex<HashSet<i64>> {
    static PERSISTENT_IDS: OnceLock<Mutex<HashSet<i64>>> = OnceLock::new();
    PERSISTENT_IDS.get_or_init(|| Mutex::new(HashSet::new()))
}

/// Returns a fresh, never-reused handle ID. IDs start at 1 so `0` and `-1`
/// remain available as "absent" / "error" sentinels.
fn next_id() -> i64 {
    static NEXT: AtomicI64 = AtomicI64::new(1);
    NEXT.fetch_add(1, Ordering::SeqCst)
}

/// Static buffer holding the last message captured by a failed `elephc_pdo_open`.
fn open_error_cell() -> &'static Mutex<CString> {
    static C: OnceLock<Mutex<CString>> = OnceLock::new();
    C.get_or_init(|| Mutex::new(CString::default()))
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

/// Static buffer for the most recent `elephc_pdo_column_decltype` result.
fn decltype_cell() -> &'static Mutex<CString> {
    static C: OnceLock<Mutex<CString>> = OnceLock::new();
    C.get_or_init(|| Mutex::new(CString::default()))
}

/// Static buffer for the most recent `elephc_pdo_column_text` result.
fn coltext_cell() -> &'static Mutex<CString> {
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

/// Static buffer for the most recent `elephc_pdo_server_version` result.
fn server_version_cell() -> &'static Mutex<CString> {
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

/// Static byte buffer for the most recent whole BLOB / large-object read
/// (`elephc_pdo_blob_read` / `elephc_pdo_lob_get`), drained byte-by-byte through
/// `elephc_pdo_blob_byte`. A `Vec<u8>` rather than a `CString` because BLOBs are
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
    let mut guard = cell.lock().unwrap();
    *guard = cstr;
    guard.as_ptr()
}

/// Stores raw bytes into the per-result static data buffer and returns a pointer
/// to the first byte, or null for an empty buffer. Valid until the next column
/// data pointer call; elephc copies it immediately through `ptr_read_string`.
fn store_bytes(bytes: Vec<u8>) -> *const c_char {
    let mut guard = coldata_cell().lock().unwrap();
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
/// (P1-10/P2-9) is only consulted for a `sqlite:` DSN; `my_init_command` (P1-9)
/// only for a `mysql:` DSN; PostgreSQL and the other driver's parameter are
/// ignored.
fn open_conn_for_dsn(dsn: &str, sqlite_open_flags: i64, my_init_command: &str) -> Result<Conn, String> {
    if let Some(path) = dsn.strip_prefix("sqlite:") {
        sqlite::SqliteConn::open(path, sqlite_open_flags).map(Conn::Sqlite)
    } else if dsn.starts_with("pgsql:") {
        pg::PgConn::open(dsn).map(Conn::Postgres)
    } else if dsn.starts_with("mysql:") {
        my::MyConn::open(dsn, my_init_command).map(Conn::Mysql)
    } else {
        Err(
            "could not find driver (only sqlite:, pgsql:, and mysql: DSNs are supported)"
                .to_string(),
        )
    }
}

/// Registers a newly opened connection and returns the public handle ID.
fn register_conn(conn: Conn) -> i64 {
    let id = next_id();
    conns().lock().unwrap().insert(id, conn);
    id
}

/// Opens a non-persistent connection and stores any failure message for the PDO
/// constructor's `elephc_pdo_last_open_error()` call.
fn open_nonpersistent_dsn(dsn: &str, sqlite_open_flags: i64, my_init_command: &str) -> i64 {
    match open_conn_for_dsn(dsn, sqlite_open_flags, my_init_command) {
        Ok(conn) => register_conn(conn),
        Err(msg) => {
            store_cstr(open_error_cell(), &msg);
            -1
        }
    }
}

/// Opens or reuses a process-local persistent connection for the full DSN.
/// `sqlite_open_flags`/`my_init_command` are only applied on a fresh open — the
/// persistent pool is keyed by DSN alone, so a later open reusing an
/// already-pooled connection does not re-apply a different flags/init-command
/// request (matching how no other constructor option retroactively affects a
/// reused persistent handle either).
fn open_persistent_dsn(dsn: &str, sqlite_open_flags: i64, my_init_command: &str) -> i64 {
    if let Some(id) = persistent_conns().lock().unwrap().get(dsn).copied() {
        if conns().lock().unwrap().contains_key(&id) {
            return id;
        }
    }
    match open_conn_for_dsn(dsn, sqlite_open_flags, my_init_command) {
        Ok(conn) => {
            let id = register_conn(conn);
            persistent_conns()
                .lock()
                .unwrap()
                .insert(dsn.to_string(), id);
            persistent_ids().lock().unwrap().insert(id);
            id
        }
        Err(msg) => {
            store_cstr(open_error_cell(), &msg);
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
/// `Pdo\Sqlite::openBlob()` / `Pdo\Pgsql::lobOpen()` (read-whole into a
/// `php://memory` stream). v13 adds the SQLite custom-collation registration
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
#[no_mangle]
pub extern "C" fn elephc_pdo_version() -> i32 {
    18
}

/// Returns a pointer to the lowercase PDO driver name for a connection
/// (`"sqlite"`, `"pgsql"`, or `"mysql"`), or an empty string for an unknown
/// handle. Backs `PDO::getAttribute(PDO::ATTR_DRIVER_NAME)`. Valid until the next
/// `elephc_pdo_driver_name`.
#[no_mangle]
pub extern "C" fn elephc_pdo_driver_name(conn_id: i64) -> *const c_char {
    let name = match conns().lock().unwrap().get(&conn_id) {
        Some(Conn::Sqlite(_)) => "sqlite",
        Some(Conn::Postgres(_)) => "pgsql",
        Some(Conn::Mysql(_)) => "mysql",
        None => "",
    };
    store_cstr(drivername_cell(), name)
}

/// Opens a non-persistent database for a PDO DSN, dispatching on the driver
/// prefix. Returns an `i64` connection handle, or `-1` on failure with the
/// message stashed for `elephc_pdo_last_open_error`.
///
/// # Safety
/// `dsn` must point to a NUL-terminated string valid for the duration of the call.
#[no_mangle]
pub unsafe extern "C" fn elephc_pdo_open(dsn: *const c_char) -> i64 {
    let Some(dsn) = cstr_arg(dsn) else {
        store_cstr(open_error_cell(), "invalid DSN");
        return -1;
    };
    open_nonpersistent_dsn(dsn, 0, "")
}

/// Opens a database for a PDO DSN, reusing a process-local pooled connection when
/// `persistent` is non-zero. Persistent handles stay registered until process
/// exit; `elephc_pdo_close` is a no-op for them. `sqlite_open_flags` (v17) is the
/// raw `sqlite3_open_v2` flags to open a `sqlite:` DSN with — `0` means "use the
/// default `READWRITE|CREATE`" — and is ignored for PostgreSQL/MySQL DSNs; it backs
/// `Pdo\Sqlite::ATTR_OPEN_FLAGS` (P1-10). `my_init_command` (v18) is a SQL
/// statement run right after authentication on a `mysql:` connection (empty = do
/// nothing), ignored for SQLite/PostgreSQL DSNs; it backs the minimal wiring for
/// `Pdo\Mysql::ATTR_INIT_COMMAND` (P1-9).
///
/// # Safety
/// `dsn` and, when non-null, `my_init_command` must each point to a
/// NUL-terminated string valid for the duration of the call.
#[no_mangle]
pub unsafe extern "C" fn elephc_pdo_open_persistent(
    dsn: *const c_char,
    persistent: i64,
    sqlite_open_flags: i64,
    my_init_command: *const c_char,
) -> i64 {
    let Some(dsn) = cstr_arg(dsn) else {
        store_cstr(open_error_cell(), "invalid DSN");
        return -1;
    };
    let init_command = cstr_arg(my_init_command).unwrap_or("");
    if persistent == 0 {
        open_nonpersistent_dsn(dsn, sqlite_open_flags, init_command)
    } else {
        open_persistent_dsn(dsn, sqlite_open_flags, init_command)
    }
}

/// Returns a pointer to the message captured by the most recent failed
/// `elephc_pdo_open`. Valid until the next failed open.
#[no_mangle]
pub extern "C" fn elephc_pdo_last_open_error() -> *const c_char {
    open_error_cell().lock().unwrap().as_ptr()
}

/// Closes a connection (finalizing any SQLite statements still registered against
/// it) and removes it from the table. Unknown handles are ignored.
#[no_mangle]
pub extern "C" fn elephc_pdo_close(conn_id: i64) {
    if persistent_ids().lock().unwrap().contains(&conn_id) {
        return;
    }
    // The SQLite db pointer of the connection being closed, so only *its*
    // statements are finalized (statements from other open SQLite connections
    // must be left alone). `None` when the connection is PostgreSQL or unknown.
    let sqlite_db = match conns().lock().unwrap().get(&conn_id) {
        Some(Conn::Sqlite(c)) => Some(c.db),
        _ => None,
    };
    // Finalize and drop the statements belonging to this connection so
    // sqlite3_close does not fail with SQLITE_BUSY; PostgreSQL/MySQL statements
    // live server-side and are dropped with the client.
    let owned: Vec<i64> = stmts()
        .lock()
        .unwrap()
        .iter()
        .filter_map(|(k, s)| match s {
            Stmt::Sqlite(st) if sqlite_db == Some(st.db) => Some(*k),
            Stmt::Postgres(p) if p.conn_id == conn_id => Some(*k),
            Stmt::Mysql(m) if m.conn_id == conn_id => Some(*k),
            _ => None,
        })
        .collect();
    {
        let mut guard = stmts().lock().unwrap();
        for k in owned {
            if let Some(Stmt::Sqlite(s)) = guard.get(&k) {
                s.finalize();
            }
            guard.remove(&k);
        }
    }
    if let Some(Conn::Sqlite(c)) = conns().lock().unwrap().get(&conn_id) {
        c.close();
    }
    conns().lock().unwrap().remove(&conn_id);
}

/// Runs one or more SQL statements with no result rows (`PDO::exec`). Returns the
/// number of rows changed, or `-1` on error.
///
/// # Safety
/// `sql` must point to a NUL-terminated string valid for the duration of the call.
#[no_mangle]
pub unsafe extern "C" fn elephc_pdo_exec(conn_id: i64, sql: *const c_char) -> i64 {
    let mut guard = conns().lock().unwrap();
    match guard.get_mut(&conn_id) {
        Some(Conn::Sqlite(c)) => c.exec(sql),
        Some(Conn::Postgres(c)) => match cstr_arg(sql) {
            Some(s) => c.exec(s),
            None => -1,
        },
        Some(Conn::Mysql(c)) => match cstr_arg(sql) {
            Some(s) => c.exec(s),
            None => -1,
        },
        None => -1,
    }
}

/// Returns the id of the most recent INSERT: the SQLite rowid, or for PostgreSQL
/// `currval(name)` when a non-empty sequence name is given else `lastval()`.
///
/// # Safety
/// `name`, when non-null, must point to a NUL-terminated string valid for the call.
#[no_mangle]
pub unsafe extern "C" fn elephc_pdo_last_insert_id(conn_id: i64, name: *const c_char) -> i64 {
    let mut guard = conns().lock().unwrap();
    match guard.get_mut(&conn_id) {
        Some(Conn::Sqlite(c)) => c.last_insert_id(),
        Some(Conn::Postgres(c)) => c.last_insert_id(cstr_arg(name)),
        Some(Conn::Mysql(c)) => c.last_insert_id(cstr_arg(name)),
        None => 0,
    }
}

/// Like `elephc_pdo_last_insert_id`, but returns a pointer to the id rendered as
/// text: PostgreSQL sequence values are not always safe to round-trip as `i64`
/// (a caller-chosen sequence can be any integer type), so text avoids a lossy or
/// failing numeric bridge; likewise (P2-2) a MySQL `BIGINT UNSIGNED`
/// AUTO_INCREMENT id can exceed `i64::MAX`. Empty string on an unknown handle or
/// error. Valid until the next `elephc_pdo_last_insert_id_text`.
///
/// # Safety
/// `name`, when non-null, must point to a NUL-terminated string valid for the call.
#[no_mangle]
pub unsafe extern "C" fn elephc_pdo_last_insert_id_text(
    conn_id: i64,
    name: *const c_char,
) -> *const c_char {
    let text = {
        let mut guard = conns().lock().unwrap();
        match guard.get_mut(&conn_id) {
            Some(Conn::Sqlite(c)) => c.last_insert_id().to_string(),
            Some(Conn::Postgres(c)) => c.last_insert_id_text(cstr_arg(name)),
            Some(Conn::Mysql(c)) => c.last_insert_id_text(cstr_arg(name)),
            None => String::new(),
        }
    };
    store_cstr(last_insert_id_text_cell(), &text)
}

/// Returns the number of rows changed by the most recent statement.
#[no_mangle]
pub extern "C" fn elephc_pdo_changes(conn_id: i64) -> i64 {
    let guard = conns().lock().unwrap();
    match guard.get(&conn_id) {
        Some(Conn::Sqlite(c)) => c.changes(),
        Some(Conn::Postgres(c)) => c.changes,
        Some(Conn::Mysql(c)) => c.changes,
        None => 0,
    }
}

/// Begins a transaction (`PDO::beginTransaction`). Returns `1`/`0`.
#[no_mangle]
pub extern "C" fn elephc_pdo_begin(conn_id: i64) -> i64 {
    let mut guard = conns().lock().unwrap();
    match guard.get_mut(&conn_id) {
        Some(Conn::Sqlite(c)) => c.exec_simple(b"BEGIN"),
        Some(Conn::Postgres(c)) => c.exec_simple("BEGIN"),
        Some(Conn::Mysql(c)) => c.exec_simple("BEGIN"),
        None => 0,
    }
}

/// Commits the active transaction (`PDO::commit`). Returns `1`/`0`.
#[no_mangle]
pub extern "C" fn elephc_pdo_commit(conn_id: i64) -> i64 {
    let mut guard = conns().lock().unwrap();
    match guard.get_mut(&conn_id) {
        Some(Conn::Sqlite(c)) => c.exec_simple(b"COMMIT"),
        Some(Conn::Postgres(c)) => c.exec_simple("COMMIT"),
        Some(Conn::Mysql(c)) => c.exec_simple("COMMIT"),
        None => 0,
    }
}

/// Rolls back the active transaction (`PDO::rollBack`). Returns `1`/`0`.
#[no_mangle]
pub extern "C" fn elephc_pdo_rollback(conn_id: i64) -> i64 {
    let mut guard = conns().lock().unwrap();
    match guard.get_mut(&conn_id) {
        Some(Conn::Sqlite(c)) => c.exec_simple(b"ROLLBACK"),
        Some(Conn::Postgres(c)) => c.exec_simple("ROLLBACK"),
        Some(Conn::Mysql(c)) => c.exec_simple("ROLLBACK"),
        None => 0,
    }
}

/// Returns the driver's result code for the connection's last operation.
#[no_mangle]
pub extern "C" fn elephc_pdo_errcode(conn_id: i64) -> i64 {
    let guard = conns().lock().unwrap();
    match guard.get(&conn_id) {
        Some(Conn::Sqlite(c)) => c.errcode(),
        Some(Conn::Postgres(c)) => c.errcode,
        Some(Conn::Mysql(c)) => c.errcode,
        None => -1,
    }
}

/// Returns a pointer to the connection's current error message. Valid until the
/// next `elephc_pdo_errmsg`.
#[no_mangle]
pub extern "C" fn elephc_pdo_errmsg(conn_id: i64) -> *const c_char {
    let msg = {
        let guard = conns().lock().unwrap();
        match guard.get(&conn_id) {
            Some(Conn::Sqlite(c)) => c.errmsg(),
            Some(Conn::Postgres(c)) => c.errmsg.clone(),
            Some(Conn::Mysql(c)) => c.errmsg.clone(),
            None => String::new(),
        }
    };
    store_cstr(errmsg_cell(), &msg)
}

/// Returns a pointer to the 5-char SQLSTATE for the connection's last operation
/// (`"00000"` on success). Unknown handles also report `"00000"` (no operation
/// has been recorded for them). Valid until the next `elephc_pdo_sqlstate`.
#[no_mangle]
pub extern "C" fn elephc_pdo_sqlstate(conn_id: i64) -> *const c_char {
    let state = {
        let guard = conns().lock().unwrap();
        match guard.get(&conn_id) {
            Some(Conn::Sqlite(c)) => c.sqlstate(),
            Some(Conn::Postgres(c)) => c.sqlstate.clone(),
            Some(Conn::Mysql(c)) => c.sqlstate.clone(),
            None => "00000".to_string(),
        }
    };
    store_cstr(sqlstate_cell(), &state)
}

/// Sets the busy-wait timeout (in milliseconds) for lock contention: SQLite calls
/// `sqlite3_busy_timeout`; PostgreSQL/MySQL have no equivalent client-side knob
/// for this bridge's one-statement-at-a-time connections, so they no-op and
/// report success. Returns `1`/`0`.
#[no_mangle]
pub extern "C" fn elephc_pdo_set_busy_timeout(conn_id: i64, ms: i64) -> i64 {
    let guard = conns().lock().unwrap();
    match guard.get(&conn_id) {
        Some(Conn::Sqlite(c)) => c.set_busy_timeout(ms),
        Some(Conn::Postgres(_)) => 1,
        Some(Conn::Mysql(_)) => 1,
        None => 0,
    }
}

/// Returns a pointer to the connection's server/library version string: SQLite's
/// bundled `sqlite3_libversion()`, or the PostgreSQL/MySQL server's reported
/// version. Empty for an unknown handle. Valid until the next
/// `elephc_pdo_server_version`.
#[no_mangle]
pub extern "C" fn elephc_pdo_server_version(conn_id: i64) -> *const c_char {
    let version = {
        let mut guard = conns().lock().unwrap();
        match guard.get_mut(&conn_id) {
            Some(Conn::Sqlite(c)) => c.server_version(),
            Some(Conn::Postgres(c)) => c.server_version(),
            Some(Conn::Mysql(c)) => c.server_version(),
            None => String::new(),
        }
    };
    store_cstr(server_version_cell(), &version)
}

/// Returns the PostgreSQL backend process id for a `pgsql:` connection (backs
/// `Pdo\Pgsql::getPid()`); 0 for a SQLite/MySQL connection or an unknown handle.
#[no_mangle]
pub extern "C" fn elephc_pdo_backend_pid(conn_id: i64) -> i64 {
    let mut guard = conns().lock().unwrap();
    match guard.get_mut(&conn_id) {
        Some(Conn::Postgres(c)) => c.backend_pid(),
        Some(Conn::Sqlite(_)) => 0,
        Some(Conn::Mysql(_)) => 0,
        None => 0,
    }
}

/// Returns the number of warnings from the last statement on a `mysql:` connection
/// (backs `Pdo\Mysql::getWarningCount()`); 0 for a SQLite/PostgreSQL connection or
/// an unknown handle.
#[no_mangle]
pub extern "C" fn elephc_pdo_warning_count(conn_id: i64) -> i64 {
    let mut guard = conns().lock().unwrap();
    match guard.get_mut(&conn_id) {
        Some(Conn::Mysql(c)) => c.warning_count(),
        Some(Conn::Sqlite(_)) => 0,
        Some(Conn::Postgres(_)) => 0,
        None => 0,
    }
}

/// Creates a large object and returns its OID as text for a `pgsql:` connection
/// (`Pdo\Pgsql::lobCreate()`); empty string for a non-PostgreSQL connection, an
/// unknown handle, or an error.
#[no_mangle]
pub extern "C" fn elephc_pdo_lob_create(conn_id: i64) -> *const c_char {
    let text = {
        let mut guard = conns().lock().unwrap();
        match guard.get_mut(&conn_id) {
            Some(Conn::Postgres(c)) => c.lob_create(),
            _ => String::new(),
        }
    };
    store_cstr(pg_text_result_cell(), &text)
}

/// Deletes a large object by OID for a `pgsql:` connection (`Pdo\Pgsql::lobUnlink()`);
/// returns 1 on success, 0 for a non-PostgreSQL connection, unknown handle, or error.
///
/// # Safety
/// `oid` must point to a NUL-terminated string valid for the call.
#[no_mangle]
pub unsafe extern "C" fn elephc_pdo_lob_unlink(conn_id: i64, oid: *const c_char) -> i64 {
    let Some(oid) = cstr_arg(oid) else {
        return 0;
    };
    let mut guard = conns().lock().unwrap();
    match guard.get_mut(&conn_id) {
        Some(Conn::Postgres(c)) => c.lob_unlink(oid),
        _ => 0,
    }
}

/// Runs a prelude-built `COPY … FROM STDIN` for a `pgsql:` connection, streaming
/// `data` into it (`Pdo\Pgsql::copyFromArray()` / `copyFromFile()`); returns the row
/// count copied, or -1 for a non-PostgreSQL connection, unknown handle, or error.
///
/// # Safety
/// `copy_sql` and `data` must point to NUL-terminated strings valid for the call.
#[no_mangle]
pub unsafe extern "C" fn elephc_pdo_copy_in(
    conn_id: i64,
    copy_sql: *const c_char,
    data: *const c_char,
) -> i64 {
    let (Some(sql), Some(data)) = (cstr_arg(copy_sql), cstr_arg(data)) else {
        return -1;
    };
    let mut guard = conns().lock().unwrap();
    match guard.get_mut(&conn_id) {
        Some(Conn::Postgres(c)) => c.copy_in(sql, data.as_bytes()),
        _ => -1,
    }
}

/// Runs a prelude-built `COPY … TO STDOUT` for a `pgsql:` connection and returns the
/// raw text output (`Pdo\Pgsql::copyToArray()` / `copyToFile()`); empty string for a
/// non-PostgreSQL connection, unknown handle, or error.
///
/// # Safety
/// `copy_sql` must point to a NUL-terminated string valid for the call.
#[no_mangle]
pub unsafe extern "C" fn elephc_pdo_copy_out(
    conn_id: i64,
    copy_sql: *const c_char,
) -> *const c_char {
    let Some(sql) = cstr_arg(copy_sql) else {
        return store_cstr(pg_text_result_cell(), "");
    };
    let text = {
        let mut guard = conns().lock().unwrap();
        match guard.get_mut(&conn_id) {
            Some(Conn::Postgres(c)) => c.copy_out(sql),
            _ => String::new(),
        }
    };
    store_cstr(pg_text_result_cell(), &text)
}

/// Polls a `pgsql:` connection for a pending LISTEN/NOTIFY notification
/// (`Pdo\Pgsql::getNotify()`), returning it as `channel\tpid\tpayload`, or an empty
/// string if none arrives within `timeout_ms` (or for a non-PostgreSQL connection /
/// unknown handle).
#[no_mangle]
pub extern "C" fn elephc_pdo_get_notify(conn_id: i64, timeout_ms: i64) -> *const c_char {
    let text = {
        let mut guard = conns().lock().unwrap();
        match guard.get_mut(&conn_id) {
            Some(Conn::Postgres(c)) => c.get_notify(timeout_ms),
            _ => String::new(),
        }
    };
    store_cstr(pg_text_result_cell(), &text)
}

/// Drains one buffered server NOTICE message from a `pgsql:` connection
/// (`Pdo\Pgsql::setNoticeCallback()`), returning its text, or an empty string when
/// none is pending (or for a non-PostgreSQL connection / unknown handle). The prelude
/// calls this in a loop after each `exec()`/`query()` and dispatches each message to
/// the registered PHP callback. The returned pointer is valid until the next
/// PostgreSQL text-returning bridge call on this thread.
#[no_mangle]
pub extern "C" fn elephc_pdo_get_notice(conn_id: i64) -> *const c_char {
    let text = {
        let guard = conns().lock().unwrap();
        match guard.get(&conn_id) {
            Some(Conn::Postgres(c)) => c.drain_notice(),
            _ => String::new(),
        }
    };
    store_cstr(pg_text_result_cell(), &text)
}

/// Reads a SQLite BLOB cell whole into the shared blob buffer (`Pdo\Sqlite::openBlob()`),
/// returning its length in bytes, or -1 for a non-SQLite connection, an unknown handle,
/// or a read error (missing row/column). The bytes are then drained with
/// `elephc_pdo_blob_byte`, which preserves embedded NUL bytes.
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
    let (Some(table), Some(column)) = (cstr_arg(table), cstr_arg(column)) else {
        return -1;
    };
    let dbname = cstr_arg(dbname).unwrap_or("main");
    let result = {
        let mut guard = conns().lock().unwrap();
        match guard.get_mut(&conn_id) {
            Some(Conn::Sqlite(c)) => c.blob_read(dbname, table, column, rowid),
            _ => return -1,
        }
    };
    match result {
        Ok(bytes) => {
            let len = bytes.len() as i64;
            *blob_cell().lock().unwrap() = bytes;
            len
        }
        Err(_) => -1,
    }
}

/// Reads a PostgreSQL large object whole into the shared blob buffer
/// (`Pdo\Pgsql::lobOpen()`), returning its length in bytes, or -1 for a
/// non-PostgreSQL connection, an unknown handle, a non-numeric OID, or a server error
/// (no such object). The bytes are then drained with `elephc_pdo_blob_byte`.
///
/// # Safety
/// `oid` must point to a NUL-terminated string valid for the call.
#[no_mangle]
pub unsafe extern "C" fn elephc_pdo_lob_get(conn_id: i64, oid: *const c_char) -> i64 {
    let Some(oid) = cstr_arg(oid) else {
        return -1;
    };
    let result = {
        let mut guard = conns().lock().unwrap();
        match guard.get_mut(&conn_id) {
            Some(Conn::Postgres(c)) => c.lob_get(oid),
            _ => return -1,
        }
    };
    match result {
        Some(bytes) => {
            let len = bytes.len() as i64;
            *blob_cell().lock().unwrap() = bytes;
            len
        }
        None => -1,
    }
}

/// Returns the byte at `offset` in the shared blob buffer populated by the most recent
/// `elephc_pdo_blob_read` / `elephc_pdo_lob_get`, or 0 when out of range. The prelude
/// drains the buffer one byte at a time (the same shape as `elephc_pdo_column_data_byte`)
/// so embedded NUL bytes survive the round-trip into a PHP string.
#[no_mangle]
pub extern "C" fn elephc_pdo_blob_byte(offset: i64) -> i64 {
    if offset < 0 {
        return 0;
    }
    let guard = blob_cell().lock().unwrap();
    guard.get(offset as usize).map(|&b| b as i64).unwrap_or(0)
}

/// Prepares a statement (`PDO::prepare` / `PDO::query`) and returns an `i64`
/// statement handle, or `-1` on a compile error.
///
/// # Safety
/// `sql` must point to a NUL-terminated string valid for the duration of the call.
#[no_mangle]
pub unsafe extern "C" fn elephc_pdo_prepare(conn_id: i64, sql: *const c_char) -> i64 {
    let prepared: Result<Stmt, ()> = {
        let mut guard = conns().lock().unwrap();
        match guard.get_mut(&conn_id) {
            Some(Conn::Sqlite(c)) => c.prepare(sql).map(Stmt::Sqlite),
            Some(Conn::Postgres(c)) => match cstr_arg(sql) {
                Some(s) => match c.prepare(s) {
                    Ok(mut st) => {
                        st.conn_id = conn_id;
                        Ok(Stmt::Postgres(st))
                    }
                    Err(_) => Err(()),
                },
                None => Err(()),
            },
            Some(Conn::Mysql(c)) => match cstr_arg(sql) {
                Some(s) => match c.prepare(s) {
                    Ok(mut st) => {
                        st.conn_id = conn_id;
                        Ok(Stmt::Mysql(st))
                    }
                    Err(_) => Err(()),
                },
                None => Err(()),
            },
            None => Err(()),
        }
    };
    match prepared {
        Ok(stmt) => {
            let id = next_id();
            stmts().lock().unwrap().insert(id, stmt);
            id
        }
        Err(()) => -1,
    }
}

/// Resolves a named placeholder to its 1-based bind index, or `0` when unknown.
///
/// # Safety
/// `name` must point to a NUL-terminated string valid for the duration of the call.
#[no_mangle]
pub unsafe extern "C" fn elephc_pdo_bind_parameter_index(stmt_id: i64, name: *const c_char) -> i64 {
    let guard = stmts().lock().unwrap();
    let Some(name) = cstr_arg(name) else {
        return 0;
    };
    match guard.get(&stmt_id) {
        Some(Stmt::Sqlite(s)) => s.bind_parameter_index(name),
        Some(Stmt::Postgres(s)) => s.bind_parameter_index(name),
        Some(Stmt::Mysql(s)) => s.bind_parameter_index(name),
        None => 0,
    }
}

/// Binds an integer to the 1-based placeholder `idx`. Returns `1`/`0`.
#[no_mangle]
pub extern "C" fn elephc_pdo_bind_int(stmt_id: i64, idx: i64, val: i64) -> i64 {
    let mut guard = stmts().lock().unwrap();
    match guard.get_mut(&stmt_id) {
        Some(Stmt::Sqlite(s)) => s.bind_int(idx, val),
        Some(Stmt::Postgres(s)) => s.bind(idx, pg::Bind::Int(val)),
        Some(Stmt::Mysql(s)) => s.bind(idx, my::Bind::Int(val)),
        None => 0,
    }
}

/// Binds a double to the 1-based placeholder `idx`. Returns `1`/`0`.
#[no_mangle]
pub extern "C" fn elephc_pdo_bind_double(stmt_id: i64, idx: i64, val: f64) -> i64 {
    let mut guard = stmts().lock().unwrap();
    match guard.get_mut(&stmt_id) {
        Some(Stmt::Sqlite(s)) => s.bind_double(idx, val),
        Some(Stmt::Postgres(s)) => s.bind(idx, pg::Bind::Float(val)),
        Some(Stmt::Mysql(s)) => s.bind(idx, my::Bind::Float(val)),
        None => 0,
    }
}

/// Binds a text value to the 1-based placeholder `idx`. A null pointer binds SQL
/// NULL. Returns `1`/`0`.
///
/// # Safety
/// `val`, when non-null, must point to a NUL-terminated string valid for the call.
#[no_mangle]
pub unsafe extern "C" fn elephc_pdo_bind_text(stmt_id: i64, idx: i64, val: *const c_char) -> i64 {
    let mut guard = stmts().lock().unwrap();
    match guard.get_mut(&stmt_id) {
        Some(Stmt::Sqlite(s)) => s.bind_text(idx, val),
        Some(Stmt::Postgres(s)) => {
            let bind = match cstr_arg(val) {
                Some(t) => pg::Bind::Text(t.to_string()),
                None => pg::Bind::Null,
            };
            s.bind(idx, bind)
        }
        Some(Stmt::Mysql(s)) => {
            let bind = match cstr_arg(val) {
                Some(t) => my::Bind::Text(t.to_string()),
                None => my::Bind::Null,
            };
            s.bind(idx, bind)
        }
        None => 0,
    }
}

/// Binds SQL NULL to the 1-based placeholder `idx`. Returns `1`/`0`.
#[no_mangle]
pub extern "C" fn elephc_pdo_bind_null(stmt_id: i64, idx: i64) -> i64 {
    let mut guard = stmts().lock().unwrap();
    match guard.get_mut(&stmt_id) {
        Some(Stmt::Sqlite(s)) => s.bind_null(idx),
        Some(Stmt::Postgres(s)) => s.bind(idx, pg::Bind::Null),
        Some(Stmt::Mysql(s)) => s.bind(idx, my::Bind::Null),
        None => 0,
    }
}

/// Binds a boolean to the 1-based placeholder `idx`: SQLite and MySQL bind it as
/// an integer `0`/`1`; PostgreSQL binds a real boolean value through the text
/// `'t'`/`'f'` parameter format PostgreSQL accepts for `bool` columns (and
/// coerces from for untyped/text columns, matching PDO/PHP's text-parameter
/// convention). Returns `1`/`0`.
#[no_mangle]
pub extern "C" fn elephc_pdo_bind_bool(stmt_id: i64, idx: i64, val: i64) -> i64 {
    let truthy = (val != 0) as i64;
    let mut guard = stmts().lock().unwrap();
    match guard.get_mut(&stmt_id) {
        Some(Stmt::Sqlite(s)) => s.bind_int(idx, truthy),
        Some(Stmt::Postgres(s)) => {
            let text = if truthy != 0 { "t" } else { "f" };
            s.bind(idx, pg::Bind::Text(text.to_string()))
        }
        Some(Stmt::Mysql(s)) => s.bind(idx, my::Bind::Int(truthy)),
        None => 0,
    }
}

/// Binds raw bytes (embedded NUL preserved) to the 1-based placeholder `idx`:
/// SQLite copies them via `SQLITE_TRANSIENT` (`sqlite3_bind_blob`); PostgreSQL and
/// MySQL bind them through each driver's raw-bytes value path (bypassing the
/// text re-encoding the other bind functions use), so arbitrary binary content
/// round-trips unchanged. Returns `1`/`0`.
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
    let mut guard = stmts().lock().unwrap();
    match guard.get_mut(&stmt_id) {
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
}

/// Resets a statement, keeping its parameter bindings. Returns `1`/`0`.
#[no_mangle]
pub extern "C" fn elephc_pdo_reset(stmt_id: i64) -> i64 {
    let mut guard = stmts().lock().unwrap();
    match guard.get_mut(&stmt_id) {
        Some(Stmt::Sqlite(s)) => s.reset(),
        Some(Stmt::Postgres(s)) => s.reset(),
        Some(Stmt::Mysql(s)) => s.reset(),
        None => 0,
    }
}

/// Clears all parameter bindings on a statement. Returns `1`/`0`.
#[no_mangle]
pub extern "C" fn elephc_pdo_clear_bindings(stmt_id: i64) -> i64 {
    let mut guard = stmts().lock().unwrap();
    match guard.get_mut(&stmt_id) {
        Some(Stmt::Sqlite(s)) => s.clear_bindings(),
        Some(Stmt::Postgres(s)) => s.clear_bindings(),
        Some(Stmt::Mysql(s)) => s.clear_bindings(),
        None => 0,
    }
}

/// Advances the statement one row: `1` for a row, `0` when exhausted, `-1` on
/// error.
#[no_mangle]
pub extern "C" fn elephc_pdo_step(stmt_id: i64) -> i64 {
    let mut sguard = stmts().lock().unwrap();
    match sguard.get_mut(&stmt_id) {
        Some(Stmt::Sqlite(s)) => s.step(),
        Some(Stmt::Postgres(s)) => {
            let conn_id = s.conn_id;
            let mut cguard = conns().lock().unwrap();
            match cguard.get_mut(&conn_id) {
                Some(Conn::Postgres(c)) => s.step(c),
                _ => -1,
            }
        }
        Some(Stmt::Mysql(s)) => {
            let conn_id = s.conn_id;
            let mut cguard = conns().lock().unwrap();
            match cguard.get_mut(&conn_id) {
                Some(Conn::Mysql(c)) => s.step(c),
                _ => -1,
            }
        }
        None => -1,
    }
}

/// Returns the number of result columns for the statement.
#[no_mangle]
pub extern "C" fn elephc_pdo_column_count(stmt_id: i64) -> i64 {
    let guard = stmts().lock().unwrap();
    match guard.get(&stmt_id) {
        Some(Stmt::Sqlite(s)) => s.column_count(),
        Some(Stmt::Postgres(s)) => s.column_count(),
        Some(Stmt::Mysql(s)) => s.column_count(),
        None => 0,
    }
}

/// Returns a pointer to the name of result column `i` (0-based).
#[no_mangle]
pub extern "C" fn elephc_pdo_column_name(stmt_id: i64, i: i64) -> *const c_char {
    let name = {
        let guard = stmts().lock().unwrap();
        match guard.get(&stmt_id) {
            Some(Stmt::Sqlite(s)) => s.column_name(i),
            Some(Stmt::Postgres(s)) => s.column_name(i),
            Some(Stmt::Mysql(s)) => s.column_name(i),
            None => String::new(),
        }
    };
    store_cstr(colname_cell(), &name)
}

/// Returns the SQLite-compatible type code for the current row's column `i`
/// (0-based): 1=int, 2=float, 3=text, 4=blob/bytea, 5=null.
#[no_mangle]
pub extern "C" fn elephc_pdo_column_type(stmt_id: i64, i: i64) -> i64 {
    let guard = stmts().lock().unwrap();
    match guard.get(&stmt_id) {
        Some(Stmt::Sqlite(s)) => s.column_type(i),
        Some(Stmt::Postgres(s)) => s.column_type(i),
        Some(Stmt::Mysql(s)) => s.column_type(i),
        None => 5,
    }
}

/// Returns a pointer to the declared type of result column `i` (0-based) for a
/// SQLite statement (`sqlite3_column_decltype`), or an empty string for a
/// non-SQLite statement or an expression column. Feeds `getColumnMeta`'s
/// native_type. Valid until the next `elephc_pdo_column_decltype`.
#[no_mangle]
pub extern "C" fn elephc_pdo_column_decltype(stmt_id: i64, i: i64) -> *const c_char {
    let decltype = {
        let guard = stmts().lock().unwrap();
        match guard.get(&stmt_id) {
            Some(Stmt::Sqlite(s)) => s.column_decltype(i),
            _ => String::new(),
        }
    };
    store_cstr(decltype_cell(), &decltype)
}

/// Loads a SQLite extension by path for a `sqlite:` connection
/// (`Pdo\Sqlite::loadExtension()`), returning 1 on success or 0 for a
/// non-SQLite connection, unknown handle, or load error.
///
/// # Safety
/// `path` must point to a NUL-terminated string valid for the call, and loading an
/// extension runs arbitrary native code from it.
#[no_mangle]
pub unsafe extern "C" fn elephc_pdo_load_extension(conn_id: i64, path: *const c_char) -> i64 {
    let Some(path) = cstr_arg(path) else {
        return 0;
    };
    let guard = conns().lock().unwrap();
    match guard.get(&conn_id) {
        Some(Conn::Sqlite(c)) => c.load_extension(path),
        _ => 0,
    }
}

/// Registers a custom SQLite collation from a compiled-PHP comparator
/// (`Pdo\Sqlite::createCollation`). `descriptor` is the callable descriptor
/// pointer and `adapter` the codegen collation-adapter address, both produced by
/// the prelude via `__elephc_callable_ptr` / `__elephc_pdo_adapter_addr`. Returns
/// `1` on success, `0` on error or a non-SQLite handle. Registration itself never
/// fires the comparator (SQLite invokes it later, during an `ORDER BY … COLLATE`),
/// so the connection lock is held only for the brief `sqlite3_create_collation_v2`.
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
    let Some(name) = cstr_arg(name) else {
        return 0;
    };
    let guard = conns().lock().unwrap();
    match guard.get(&conn_id) {
        Some(Conn::Sqlite(c)) => c.create_collation(name, descriptor, adapter as *const c_void),
        _ => 0,
    }
}

/// Registers a scalar SQL function `name` backed by a compiled-PHP callable
/// (`Pdo\Sqlite::createFunction`). `num_args` is the declared arity (-1 = variadic),
/// `flags` an optional `SQLITE_DETERMINISTIC`, and `descriptor`/`adapter` the callable
/// descriptor pointer and the codegen scalar adapter address, produced by the prelude
/// via `__elephc_callable_ptr` / `__elephc_pdo_adapter_addr`. Returns `1` on success,
/// `0` on error or a non-SQLite handle.
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
    let Some(name) = cstr_arg(name) else {
        return 0;
    };
    let guard = conns().lock().unwrap();
    match guard.get(&conn_id) {
        Some(Conn::Sqlite(c)) => {
            c.create_function(name, num_args, flags, descriptor, adapter as *const c_void)
        }
        _ => 0,
    }
}

/// Registers an aggregate SQL function `name` backed by a compiled-PHP step +
/// finalize pair (`Pdo\Sqlite::createAggregate`). `num_args` is the declared arity
/// (-1 = variadic); each callable crosses as a (descriptor, adapter) pointer pair,
/// produced by the prelude via `__elephc_callable_ptr` / `__elephc_pdo_adapter_addr`
/// (kinds 2 and 3). Returns `1` on success, `0` on error or a non-SQLite handle.
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
    let Some(name) = cstr_arg(name) else {
        return 0;
    };
    let guard = conns().lock().unwrap();
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
}

/// Returns the current row's column `i` (0-based) as an integer.
#[no_mangle]
pub extern "C" fn elephc_pdo_column_int(stmt_id: i64, i: i64) -> i64 {
    let guard = stmts().lock().unwrap();
    match guard.get(&stmt_id) {
        Some(Stmt::Sqlite(s)) => s.column_int(i),
        Some(Stmt::Postgres(s)) => s.column_int(i),
        Some(Stmt::Mysql(s)) => s.column_int(i),
        None => 0,
    }
}

/// Returns the current row's column `i` (0-based) as a double.
#[no_mangle]
pub extern "C" fn elephc_pdo_column_double(stmt_id: i64, i: i64) -> f64 {
    let guard = stmts().lock().unwrap();
    match guard.get(&stmt_id) {
        Some(Stmt::Sqlite(s)) => s.column_double(i),
        Some(Stmt::Postgres(s)) => s.column_double(i),
        Some(Stmt::Mysql(s)) => s.column_double(i),
        None => 0.0,
    }
}

/// Returns a pointer to the current row's column `i` (0-based) text.
#[no_mangle]
pub extern "C" fn elephc_pdo_column_text(stmt_id: i64, i: i64) -> *const c_char {
    let text = {
        let guard = stmts().lock().unwrap();
        match guard.get(&stmt_id) {
            Some(Stmt::Sqlite(s)) => s.column_text(i),
            Some(Stmt::Postgres(s)) => s.column_text(i),
            Some(Stmt::Mysql(s)) => s.column_text(i),
            None => String::new(),
        }
    };
    store_cstr(coltext_cell(), &text)
}

/// Returns the byte length of the current row's column `i` rendered as PDO text
/// or BLOB bytes. Unlike `elephc_pdo_column_text`, this path preserves embedded
/// NUL bytes when paired with `elephc_pdo_column_data_ptr`.
#[no_mangle]
pub extern "C" fn elephc_pdo_column_data_len(stmt_id: i64, i: i64) -> i64 {
    let guard = stmts().lock().unwrap();
    match guard.get(&stmt_id) {
        Some(Stmt::Sqlite(s)) => s.column_data(i).len() as i64,
        Some(Stmt::Postgres(s)) => s.column_data(i).len() as i64,
        Some(Stmt::Mysql(s)) => s.column_data(i).len() as i64,
        None => 0,
    }
}

/// Returns a pointer to the current row's column `i` rendered as raw bytes.
/// The pointer remains valid until the next `elephc_pdo_column_data_ptr` call.
#[no_mangle]
pub extern "C" fn elephc_pdo_column_data_ptr(stmt_id: i64, i: i64) -> *const c_char {
    let bytes = {
        let guard = stmts().lock().unwrap();
        match guard.get(&stmt_id) {
            Some(Stmt::Sqlite(s)) => s.column_data(i),
            Some(Stmt::Postgres(s)) => s.column_data(i),
            Some(Stmt::Mysql(s)) => s.column_data(i),
            None => Vec::new(),
        }
    };
    store_bytes(bytes)
}

/// Returns one byte from the current row's column `i` rendered as raw data.
/// Out-of-range handles, columns, and offsets return `0`.
#[no_mangle]
pub extern "C" fn elephc_pdo_column_data_byte(stmt_id: i64, i: i64, offset: i64) -> i64 {
    let Ok(offset) = usize::try_from(offset) else {
        return 0;
    };
    let bytes = {
        let guard = stmts().lock().unwrap();
        match guard.get(&stmt_id) {
            Some(Stmt::Sqlite(s)) => s.column_data(i),
            Some(Stmt::Postgres(s)) => s.column_data(i),
            Some(Stmt::Mysql(s)) => s.column_data(i),
            None => Vec::new(),
        }
    };
    bytes.get(offset).copied().unwrap_or(0) as i64
}

/// Finalizes a statement and removes it from the table. Unknown handles return
/// `0`; success returns `1`.
#[no_mangle]
pub extern "C" fn elephc_pdo_finalize(stmt_id: i64) -> i64 {
    match stmts().lock().unwrap().remove(&stmt_id) {
        Some(Stmt::Sqlite(s)) => {
            s.finalize();
            1
        }
        Some(Stmt::Postgres(_)) => 1,
        Some(Stmt::Mysql(_)) => 1,
        None => 0,
    }
}

/// Returns `1` if a SQLite statement makes no direct changes to the database file
/// content (`sqlite3_stmt_readonly`), else `0` — including for a non-SQLite or
/// unknown handle, where the notion does not apply. Backs
/// `PDOStatement::getAttribute(Pdo\Sqlite::ATTR_READONLY_STATEMENT)` (P2-16) as a
/// live read rather than a value stored at prepare time.
#[no_mangle]
pub extern "C" fn elephc_pdo_stmt_readonly(stmt_id: i64) -> i64 {
    match stmts().lock().unwrap().get(&stmt_id) {
        Some(Stmt::Sqlite(s)) => s.readonly(),
        _ => 0,
    }
}

/// Returns the native driver code for the statement's last operation. SQLite
/// tracks this per-connection (mirrored here from the statement's own `db`
/// pointer); PostgreSQL/MySQL statements share their connection's bookkeeping
/// (looked up by the statement's `conn_id`, the same way `elephc_pdo_step`
/// dispatches into the connection to execute). Unknown handles return `-1`.
#[no_mangle]
pub extern "C" fn elephc_pdo_stmt_errcode(stmt_id: i64) -> i64 {
    let sguard = stmts().lock().unwrap();
    match sguard.get(&stmt_id) {
        Some(Stmt::Sqlite(s)) => s.errcode(),
        Some(Stmt::Postgres(s)) => {
            let conn_id = s.conn_id;
            let cguard = conns().lock().unwrap();
            match cguard.get(&conn_id) {
                Some(Conn::Postgres(c)) => c.errcode,
                _ => -1,
            }
        }
        Some(Stmt::Mysql(s)) => {
            let conn_id = s.conn_id;
            let cguard = conns().lock().unwrap();
            match cguard.get(&conn_id) {
                Some(Conn::Mysql(c)) => c.errcode,
                _ => -1,
            }
        }
        None => -1,
    }
}

/// Returns a pointer to the statement's last error message (see
/// `elephc_pdo_stmt_errcode` for how PostgreSQL/MySQL statements share their
/// connection's bookkeeping). Empty string for an unknown handle. Valid until the
/// next `elephc_pdo_stmt_errmsg`.
#[no_mangle]
pub extern "C" fn elephc_pdo_stmt_errmsg(stmt_id: i64) -> *const c_char {
    let msg = {
        let sguard = stmts().lock().unwrap();
        match sguard.get(&stmt_id) {
            Some(Stmt::Sqlite(s)) => s.errmsg(),
            Some(Stmt::Postgres(s)) => {
                let conn_id = s.conn_id;
                let cguard = conns().lock().unwrap();
                match cguard.get(&conn_id) {
                    Some(Conn::Postgres(c)) => c.errmsg.clone(),
                    _ => String::new(),
                }
            }
            Some(Stmt::Mysql(s)) => {
                let conn_id = s.conn_id;
                let cguard = conns().lock().unwrap();
                match cguard.get(&conn_id) {
                    Some(Conn::Mysql(c)) => c.errmsg.clone(),
                    _ => String::new(),
                }
            }
            None => String::new(),
        }
    };
    store_cstr(stmt_errmsg_cell(), &msg)
}

/// Returns a pointer to the 5-char SQLSTATE for the statement's last operation
/// (see `elephc_pdo_stmt_errcode` for how PostgreSQL/MySQL statements share their
/// connection's bookkeeping). Unknown handles report `"00000"`; a statement whose
/// connection has since closed reports `"HY000"`. Valid until the next
/// `elephc_pdo_stmt_sqlstate`.
#[no_mangle]
pub extern "C" fn elephc_pdo_stmt_sqlstate(stmt_id: i64) -> *const c_char {
    let state = {
        let sguard = stmts().lock().unwrap();
        match sguard.get(&stmt_id) {
            Some(Stmt::Sqlite(s)) => s.sqlstate(),
            Some(Stmt::Postgres(s)) => {
                let conn_id = s.conn_id;
                let cguard = conns().lock().unwrap();
                match cguard.get(&conn_id) {
                    Some(Conn::Postgres(c)) => c.sqlstate.clone(),
                    _ => "HY000".to_string(),
                }
            }
            Some(Stmt::Mysql(s)) => {
                let conn_id = s.conn_id;
                let cguard = conns().lock().unwrap();
                match cguard.get(&conn_id) {
                    Some(Conn::Mysql(c)) => c.sqlstate.clone(),
                    _ => "HY000".to_string(),
                }
            }
            None => "00000".to_string(),
        }
    };
    store_cstr(stmt_sqlstate_cell(), &state)
}

#[cfg(test)]
mod tests {
    use super::*;

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
    /// history is enumerated on `elephc_pdo_version`'s own docblock. v18 adds
    /// `elephc_pdo_open_persistent`'s `my_init_command` parameter (P1-9).
    #[test]
    fn version_is_v18() {
        assert_eq!(elephc_pdo_version(), 18);
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
        let stmt = unsafe { elephc_pdo_prepare(conn, sql.as_ptr()) };
        assert!(stmt > 0, "prepare failed");
        assert_eq!(elephc_pdo_bind_int(stmt, 1, 1), 1);

        assert_eq!(elephc_pdo_step(stmt), 1);
        assert_eq!(elephc_pdo_column_count(stmt), 3);
        assert_eq!(elephc_pdo_column_int(stmt, 0), 1);
        assert_eq!(unsafe { read(elephc_pdo_column_name(stmt, 1)) }, "name");
        assert_eq!(unsafe { read(elephc_pdo_column_text(stmt, 1)) }, "Alice");
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
        let stmt = unsafe { elephc_pdo_prepare(conn, sql.as_ptr()) };
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

    /// SQLite persistent opens reuse a process-local connection by DSN and a
    /// close call leaves that pooled connection available to the next open.
    #[test]
    fn sqlite_persistent_pool_reuses_connection_after_close() {
        let dsn = cs("sqlite::memory:");
        let first = unsafe { elephc_pdo_open_persistent(dsn.as_ptr(), 1, 0, std::ptr::null()) };
        assert!(first > 0, "open failed");

        let ddl = cs("CREATE TABLE persistent_pool (n INTEGER)");
        assert_eq!(unsafe { elephc_pdo_exec(first, ddl.as_ptr()) }, 0);
        let ins = cs("INSERT INTO persistent_pool VALUES (77)");
        assert_eq!(unsafe { elephc_pdo_exec(first, ins.as_ptr()) }, 1);
        elephc_pdo_close(first);

        let second = unsafe { elephc_pdo_open_persistent(dsn.as_ptr(), 1, 0, std::ptr::null()) };
        assert_eq!(second, first);
        let sql = cs("SELECT n FROM persistent_pool");
        let stmt = unsafe { elephc_pdo_prepare(second, sql.as_ptr()) };
        assert!(stmt > 0, "prepare failed");
        assert_eq!(elephc_pdo_step(stmt), 1);
        assert_eq!(elephc_pdo_column_int(stmt, 0), 77);
        assert_eq!(elephc_pdo_finalize(stmt), 1);
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
        let stmt = unsafe { elephc_pdo_prepare(conn, ins.as_ptr()) };
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
        let q = unsafe { elephc_pdo_prepare(conn, sel.as_ptr()) };
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
        let dup_stmt = unsafe { elephc_pdo_prepare(conn, dup.as_ptr()) };
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
        let rw = unsafe { elephc_pdo_open_persistent(dsn.as_ptr(), 0, 0, std::ptr::null()) };
        assert!(rw > 0, "read-write open failed");
        let ddl = cs("CREATE TABLE t (n INTEGER)");
        assert_eq!(unsafe { elephc_pdo_exec(rw, ddl.as_ptr()) }, 0);
        elephc_pdo_close(rw);

        // Reopening with sqlite_open_flags=1 (SQLITE_OPEN_READONLY) must reject a
        // write against the now-existing file.
        let ro = unsafe { elephc_pdo_open_persistent(dsn.as_ptr(), 0, 1, std::ptr::null()) };
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
        let id = unsafe { elephc_pdo_open_persistent(dsn.as_ptr(), 0, 0, std::ptr::null()) };
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
        let sel_stmt = unsafe { elephc_pdo_prepare(conn, sel.as_ptr()) };
        assert!(sel_stmt > 0, "prepare failed");
        assert_eq!(elephc_pdo_stmt_readonly(sel_stmt), 1);

        let ins = cs("INSERT INTO t VALUES (1)");
        let ins_stmt = unsafe { elephc_pdo_prepare(conn, ins.as_ptr()) };
        assert!(ins_stmt > 0, "prepare failed");
        assert_eq!(elephc_pdo_stmt_readonly(ins_stmt), 0);

        assert_eq!(elephc_pdo_stmt_readonly(999_999), 0);

        assert_eq!(elephc_pdo_finalize(sel_stmt), 1);
        assert_eq!(elephc_pdo_finalize(ins_stmt), 1);
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
            (ffi::SQLITE_NOLFS, "IM001"),
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
        let (sql, map) = pg::translate_placeholders(
            "SELECT * FROM t WHERE a = ? AND b = :b AND c = :b AND d = 'x?:y' AND e = id::text",
        );
        assert_eq!(
            sql,
            "SELECT * FROM t WHERE a = $1 AND b = $2 AND c = $2 AND d = 'x?:y' AND e = id::text"
        );
        assert_eq!(map.get("b"), Some(&2));
    }

    /// A `pgsql:` DSN parses into a libpq connection string.
    #[test]
    fn pg_dsn_parses() {
        let s = pg::parse_dsn("pgsql:host=localhost;port=5432;dbname=app").unwrap();
        assert!(s.contains("host='localhost'"), "got: {s}");
        assert!(s.contains("dbname='app'"), "got: {s}");
    }

    /// Full PostgreSQL round-trip against a live server. Ignored by default; run
    /// with `ELEPHC_PG_TEST_DSN` set, e.g.
    /// `ELEPHC_PG_TEST_DSN='pgsql:host=localhost;port=55432;dbname=testdb;user=test;password=test'`.
    /// Also covers the v7 additions: `elephc_pdo_bind_bool`, `elephc_pdo_bind_blob`
    /// (an embedded-NUL blob and a null-pointer→NULL bind), `elephc_pdo_sqlstate`/
    /// `elephc_pdo_stmt_sqlstate` (`"00000"` after a success, a real SQLSTATE after
    /// a forced duplicate-key error, and a reset back to `"00000"` after the next
    /// successful `prepare()`), and `elephc_pdo_server_version`.
    #[test]
    #[ignore]
    fn pg_round_trip() {
        let Ok(dsn) = std::env::var("ELEPHC_PG_TEST_DSN") else {
            return;
        };
        let dsn = cs(&dsn);
        let conn = unsafe { elephc_pdo_open(dsn.as_ptr()) };
        assert!(conn > 0, "pg open failed");

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
        let stmt = unsafe { elephc_pdo_prepare(conn, ins.as_ptr()) };
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
        unsafe { elephc_pdo_bind_text(stmt, ni, ada.as_ptr()) };
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
        let stmt2 = unsafe { elephc_pdo_prepare(conn, ins2.as_ptr()) };
        assert!(stmt2 > 0, "pg prepare failed");
        let ni2 = unsafe { elephc_pdo_bind_parameter_index(stmt2, n.as_ptr()) };
        let si2 = unsafe { elephc_pdo_bind_parameter_index(stmt2, s.as_ptr()) };
        let fi2 = unsafe { elephc_pdo_bind_parameter_index(stmt2, f.as_ptr()) };
        let di2 = unsafe { elephc_pdo_bind_parameter_index(stmt2, d.as_ptr()) };
        let grace = cs("Grace");
        unsafe { elephc_pdo_bind_text(stmt2, ni2, grace.as_ptr()) };
        elephc_pdo_bind_double(stmt2, si2, 1.0);
        elephc_pdo_bind_bool(stmt2, fi2, 0);
        unsafe { elephc_pdo_bind_blob(stmt2, di2, std::ptr::null(), 0) };
        assert_eq!(elephc_pdo_step(stmt2), 0);
        elephc_pdo_finalize(stmt2);

        let sel = cs("SELECT id, name, score, flag, data FROM pdo_rt WHERE id = ?");
        let q = unsafe { elephc_pdo_prepare(conn, sel.as_ptr()) };
        elephc_pdo_bind_int(q, 1, 1);
        assert_eq!(elephc_pdo_step(q), 1);
        assert_eq!(elephc_pdo_column_int(q, 0), 1);
        assert_eq!(unsafe { read(elephc_pdo_column_name(q, 1)) }, "name");
        assert_eq!(unsafe { read(elephc_pdo_column_text(q, 1)) }, "Ada");
        assert_eq!(elephc_pdo_column_double(q, 2), 9.5);
        assert_eq!(elephc_pdo_column_int(q, 3), 1);
        assert_eq!(elephc_pdo_column_data_len(q, 4), 3);
        let ptr = elephc_pdo_column_data_ptr(q, 4);
        assert_eq!(unsafe { read_bytes(ptr, 3) }, b"A\0B");
        assert_eq!(elephc_pdo_step(q), 0);
        elephc_pdo_finalize(q);

        let sel2 = cs("SELECT data FROM pdo_rt WHERE id = 2");
        let q2 = unsafe { elephc_pdo_prepare(conn, sel2.as_ptr()) };
        assert!(q2 > 0, "pg prepare failed");
        assert_eq!(elephc_pdo_step(q2), 1);
        assert_eq!(elephc_pdo_column_type(q2, 0), 5, "null-pointer blob bind must read back as NULL");
        elephc_pdo_finalize(q2);

        // Bug 2 regression coverage: a forced duplicate-key error reports a
        // non-"00000" SQLSTATE at both the connection and statement level, and
        // the following successful prepare() resets it back to "00000".
        let dup = cs("INSERT INTO pdo_rt (id, name) VALUES (1, 'dup')");
        let dup_stmt = unsafe { elephc_pdo_prepare(conn, dup.as_ptr()) };
        assert!(dup_stmt > 0, "pg prepare failed");
        assert_eq!(elephc_pdo_step(dup_stmt), -1);
        let dup_state = unsafe { read(elephc_pdo_sqlstate(conn)) };
        assert_ne!(dup_state, "00000", "expected a real SQLSTATE, got: {dup_state}");
        assert_eq!(unsafe { read(elephc_pdo_stmt_sqlstate(dup_stmt)) }, dup_state);
        elephc_pdo_finalize(dup_stmt);

        let sel3 = cs("SELECT 1");
        let ok_stmt = unsafe { elephc_pdo_prepare(conn, sel3.as_ptr()) };
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
        let (sql, map, order) = my::translate_placeholders(
            "SELECT * FROM t WHERE a = ? AND b = :b AND c = :b AND d = 'x?:y' AND e = id::text",
        );
        assert_eq!(
            sql,
            "SELECT * FROM t WHERE a = ? AND b = ? AND c = ? AND d = 'x?:y' AND e = id::text"
        );
        // `?`→slot 1, `:b`→slot 2 (reused for the second `:b`).
        assert_eq!(order, vec![1, 2, 2]);
        assert_eq!(map.get("b"), Some(&2));
    }

    /// Full MySQL/MariaDB round-trip against a live server. Ignored by default; run
    /// with `ELEPHC_MY_TEST_DSN` set, e.g.
    /// `ELEPHC_MY_TEST_DSN='mysql:host=localhost;port=33060;dbname=testdb;user=test;password=test'`.
    /// Also covers the v7 additions: `elephc_pdo_bind_bool`, `elephc_pdo_bind_blob`
    /// (an embedded-NUL blob and a null-pointer→NULL bind), `elephc_pdo_sqlstate`/
    /// `elephc_pdo_stmt_sqlstate` (`"00000"` after a success, a real SQLSTATE after
    /// a forced duplicate-key error, and a reset back to `"00000"` after the next
    /// successful `prepare()`), and `elephc_pdo_server_version`.
    #[test]
    #[ignore]
    fn my_round_trip() {
        let Ok(dsn) = std::env::var("ELEPHC_MY_TEST_DSN") else {
            return;
        };
        let dsn = cs(&dsn);
        let conn = unsafe { elephc_pdo_open(dsn.as_ptr()) };
        assert!(conn > 0, "mysql open failed");
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
        let stmt = unsafe { elephc_pdo_prepare(conn, ins.as_ptr()) };
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
        unsafe { elephc_pdo_bind_text(stmt, ni, ada.as_ptr()) };
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
        let stmt2 = unsafe { elephc_pdo_prepare(conn, ins2.as_ptr()) };
        assert!(stmt2 > 0, "mysql prepare failed");
        let ni2 = unsafe { elephc_pdo_bind_parameter_index(stmt2, n.as_ptr()) };
        let si2 = unsafe { elephc_pdo_bind_parameter_index(stmt2, s.as_ptr()) };
        let fi2 = unsafe { elephc_pdo_bind_parameter_index(stmt2, f.as_ptr()) };
        let di2 = unsafe { elephc_pdo_bind_parameter_index(stmt2, d.as_ptr()) };
        let grace = cs("Grace");
        unsafe { elephc_pdo_bind_text(stmt2, ni2, grace.as_ptr()) };
        elephc_pdo_bind_double(stmt2, si2, 1.0);
        elephc_pdo_bind_bool(stmt2, fi2, 0);
        unsafe { elephc_pdo_bind_blob(stmt2, di2, std::ptr::null(), 0) };
        assert_eq!(elephc_pdo_step(stmt2), 0);
        elephc_pdo_finalize(stmt2);

        let sel = cs("SELECT id, name, score, flag, data FROM pdo_rt WHERE id = ?");
        let q = unsafe { elephc_pdo_prepare(conn, sel.as_ptr()) };
        elephc_pdo_bind_int(q, 1, 1);
        assert_eq!(elephc_pdo_step(q), 1);
        assert_eq!(elephc_pdo_column_int(q, 0), 1);
        assert_eq!(unsafe { read(elephc_pdo_column_name(q, 1)) }, "name");
        assert_eq!(unsafe { read(elephc_pdo_column_text(q, 1)) }, "Ada");
        assert_eq!(elephc_pdo_column_double(q, 2), 9.5);
        assert_eq!(elephc_pdo_column_int(q, 3), 1);
        assert_eq!(elephc_pdo_column_data_len(q, 4), 3);
        let ptr = elephc_pdo_column_data_ptr(q, 4);
        assert_eq!(unsafe { read_bytes(ptr, 3) }, b"A\0B");
        assert_eq!(elephc_pdo_step(q), 0);
        elephc_pdo_finalize(q);

        let sel2 = cs("SELECT data FROM pdo_rt WHERE id = 2");
        let q2 = unsafe { elephc_pdo_prepare(conn, sel2.as_ptr()) };
        assert!(q2 > 0, "mysql prepare failed");
        assert_eq!(elephc_pdo_step(q2), 1);
        assert_eq!(elephc_pdo_column_type(q2, 0), 5, "null-pointer blob bind must read back as NULL");
        elephc_pdo_finalize(q2);

        // Bug 2 regression coverage: a forced duplicate-key error reports a
        // non-"00000" SQLSTATE at both the connection and statement level, and
        // the following successful prepare() resets it back to "00000".
        let dup = cs("INSERT INTO pdo_rt (id, name) VALUES (1, 'dup')");
        let dup_stmt = unsafe { elephc_pdo_prepare(conn, dup.as_ptr()) };
        assert!(dup_stmt > 0, "mysql prepare failed");
        assert_eq!(elephc_pdo_step(dup_stmt), -1);
        let dup_state = unsafe { read(elephc_pdo_sqlstate(conn)) };
        assert_ne!(dup_state, "00000", "expected a real SQLSTATE, got: {dup_state}");
        assert_eq!(unsafe { read(elephc_pdo_stmt_sqlstate(dup_stmt)) }, dup_state);
        elephc_pdo_finalize(dup_stmt);

        let sel3 = cs("SELECT 1");
        let ok_stmt = unsafe { elephc_pdo_prepare(conn, sel3.as_ptr()) };
        assert!(ok_stmt > 0, "mysql prepare failed");
        assert_eq!(unsafe { read(elephc_pdo_sqlstate(conn)) }, "00000");
        elephc_pdo_finalize(ok_stmt);

        let cleanup = cs("DROP TABLE pdo_rt");
        unsafe { elephc_pdo_exec(conn, cleanup.as_ptr()) };
        elephc_pdo_close(conn);
    }
}
