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
use std::os::raw::c_char;
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

/// Opens the driver connection for a validated DSN string.
fn open_conn_for_dsn(dsn: &str) -> Result<Conn, String> {
    if let Some(path) = dsn.strip_prefix("sqlite:") {
        sqlite::SqliteConn::open(path).map(Conn::Sqlite)
    } else if dsn.starts_with("pgsql:") {
        pg::PgConn::open(dsn).map(Conn::Postgres)
    } else if dsn.starts_with("mysql:") {
        my::MyConn::open(dsn).map(Conn::Mysql)
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
fn open_nonpersistent_dsn(dsn: &str) -> i64 {
    match open_conn_for_dsn(dsn) {
        Ok(conn) => register_conn(conn),
        Err(msg) => {
            store_cstr(open_error_cell(), &msg);
            -1
        }
    }
}

/// Opens or reuses a process-local persistent connection for the full DSN.
fn open_persistent_dsn(dsn: &str) -> i64 {
    if let Some(id) = persistent_conns().lock().unwrap().get(dsn).copied() {
        if conns().lock().unwrap().contains_key(&id) {
            return id;
        }
    }
    match open_conn_for_dsn(dsn) {
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

/// Returns the bridge ABI version. Bumped when the C ABI shape changes.
#[no_mangle]
pub extern "C" fn elephc_pdo_version() -> i32 {
    6
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
    open_nonpersistent_dsn(dsn)
}

/// Opens a database for a PDO DSN, reusing a process-local pooled connection when
/// `persistent` is non-zero. Persistent handles stay registered until process
/// exit; `elephc_pdo_close` is a no-op for them.
///
/// # Safety
/// `dsn` must point to a NUL-terminated string valid for the duration of the call.
#[no_mangle]
pub unsafe extern "C" fn elephc_pdo_open_persistent(
    dsn: *const c_char,
    persistent: i64,
) -> i64 {
    let Some(dsn) = cstr_arg(dsn) else {
        store_cstr(open_error_cell(), "invalid DSN");
        return -1;
    };
    if persistent == 0 {
        open_nonpersistent_dsn(dsn)
    } else {
        open_persistent_dsn(dsn)
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

    /// The ABI version constant is the v6 (sqlite + pgsql + mysql + raw data +
    /// persistent open) surface.
    #[test]
    fn version_is_v6() {
        assert_eq!(elephc_pdo_version(), 6);
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
        let first = unsafe { elephc_pdo_open_persistent(dsn.as_ptr(), 1) };
        assert!(first > 0, "open failed");

        let ddl = cs("CREATE TABLE persistent_pool (n INTEGER)");
        assert_eq!(unsafe { elephc_pdo_exec(first, ddl.as_ptr()) }, 0);
        let ins = cs("INSERT INTO persistent_pool VALUES (77)");
        assert_eq!(unsafe { elephc_pdo_exec(first, ins.as_ptr()) }, 1);
        elephc_pdo_close(first);

        let second = unsafe { elephc_pdo_open_persistent(dsn.as_ptr(), 1) };
        assert_eq!(second, first);
        let sql = cs("SELECT n FROM persistent_pool");
        let stmt = unsafe { elephc_pdo_prepare(second, sql.as_ptr()) };
        assert!(stmt > 0, "prepare failed");
        assert_eq!(elephc_pdo_step(stmt), 1);
        assert_eq!(elephc_pdo_column_int(stmt, 0), 77);
        assert_eq!(elephc_pdo_finalize(stmt), 1);
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
    #[test]
    #[ignore]
    fn pg_round_trip() {
        let Ok(dsn) = std::env::var("ELEPHC_PG_TEST_DSN") else {
            return;
        };
        let dsn = cs(&dsn);
        let conn = unsafe { elephc_pdo_open(dsn.as_ptr()) };
        assert!(conn > 0, "pg open failed");

        let drop = cs("DROP TABLE IF EXISTS pdo_rt");
        unsafe { elephc_pdo_exec(conn, drop.as_ptr()) };
        let ddl =
            cs("CREATE TABLE pdo_rt (id SERIAL PRIMARY KEY, name TEXT, score DOUBLE PRECISION)");
        unsafe { elephc_pdo_exec(conn, ddl.as_ptr()) };

        let ins = cs("INSERT INTO pdo_rt (name, score) VALUES (:n, :s)");
        let stmt = unsafe { elephc_pdo_prepare(conn, ins.as_ptr()) };
        assert!(stmt > 0, "pg prepare failed");
        let n = cs(":n");
        let ni = unsafe { elephc_pdo_bind_parameter_index(stmt, n.as_ptr()) };
        let s = cs(":s");
        let si = unsafe { elephc_pdo_bind_parameter_index(stmt, s.as_ptr()) };
        let ada = cs("Ada");
        unsafe { elephc_pdo_bind_text(stmt, ni, ada.as_ptr()) };
        elephc_pdo_bind_double(stmt, si, 9.5);
        assert_eq!(elephc_pdo_step(stmt), 0);
        elephc_pdo_finalize(stmt);

        let lid = unsafe { elephc_pdo_last_insert_id(conn, std::ptr::null()) };
        assert_eq!(lid, 1);

        let sel = cs("SELECT id, name, score FROM pdo_rt WHERE id = ?");
        let q = unsafe { elephc_pdo_prepare(conn, sel.as_ptr()) };
        elephc_pdo_bind_int(q, 1, 1);
        assert_eq!(elephc_pdo_step(q), 1);
        assert_eq!(elephc_pdo_column_int(q, 0), 1);
        assert_eq!(unsafe { read(elephc_pdo_column_name(q, 1)) }, "name");
        assert_eq!(unsafe { read(elephc_pdo_column_text(q, 1)) }, "Ada");
        assert_eq!(elephc_pdo_column_double(q, 2), 9.5);
        assert_eq!(elephc_pdo_step(q), 0);
        elephc_pdo_finalize(q);

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

        let drop = cs("DROP TABLE IF EXISTS pdo_rt");
        unsafe { elephc_pdo_exec(conn, drop.as_ptr()) };
        let ddl = cs(
            "CREATE TABLE pdo_rt (id INTEGER PRIMARY KEY AUTO_INCREMENT, name TEXT, score DOUBLE)",
        );
        unsafe { elephc_pdo_exec(conn, ddl.as_ptr()) };

        let ins = cs("INSERT INTO pdo_rt (name, score) VALUES (:n, :s)");
        let stmt = unsafe { elephc_pdo_prepare(conn, ins.as_ptr()) };
        assert!(stmt > 0, "mysql prepare failed");
        let n = cs(":n");
        let ni = unsafe { elephc_pdo_bind_parameter_index(stmt, n.as_ptr()) };
        let s = cs(":s");
        let si = unsafe { elephc_pdo_bind_parameter_index(stmt, s.as_ptr()) };
        let ada = cs("Ada");
        unsafe { elephc_pdo_bind_text(stmt, ni, ada.as_ptr()) };
        elephc_pdo_bind_double(stmt, si, 9.5);
        assert_eq!(elephc_pdo_step(stmt), 0);
        elephc_pdo_finalize(stmt);

        let lid = unsafe { elephc_pdo_last_insert_id(conn, std::ptr::null()) };
        assert_eq!(lid, 1);

        let sel = cs("SELECT id, name, score FROM pdo_rt WHERE id = ?");
        let q = unsafe { elephc_pdo_prepare(conn, sel.as_ptr()) };
        elephc_pdo_bind_int(q, 1, 1);
        assert_eq!(elephc_pdo_step(q), 1);
        assert_eq!(elephc_pdo_column_int(q, 0), 1);
        assert_eq!(unsafe { read(elephc_pdo_column_name(q, 1)) }, "name");
        assert_eq!(unsafe { read(elephc_pdo_column_text(q, 1)) }, "Ada");
        assert_eq!(elephc_pdo_column_double(q, 2), 9.5);
        assert_eq!(elephc_pdo_step(q), 0);
        elephc_pdo_finalize(q);

        let cleanup = cs("DROP TABLE pdo_rt");
        unsafe { elephc_pdo_exec(conn, cleanup.as_ptr()) };
        elephc_pdo_close(conn);
    }
}
