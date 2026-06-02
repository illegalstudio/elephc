//! Purpose:
//! SQLite bridge surface for the elephc PDO implementation (`pdo_sqlite`).
//! Wraps the bundled SQLite C library behind a small, stable C ABI that the
//! elephc PDO prelude calls through `extern "elephc_sqlite"` declarations, so a
//! database is driven with one extern call per operation rather than with
//! hand-written assembly.
//!
//! Called from:
//! - Compiled PHP programs that use PDO, via the elephc-PHP prelude's `extern`
//!   declarations (`src/pdo_prelude.rs`). The symbols are only referenced by
//!   PDO-using programs, so non-PDO binaries never link `-lelephc_sqlite`.
//! - Rust unit tests in this crate (`cargo test -p elephc-sqlite`).
//!
//! Key details:
//! - Two global handle tables index live connections / statements by `i64` IDs
//!   so the C ABI never exposes raw pointers. Callers serialise on the table
//!   mutexes; elephc programs are effectively single-threaded, so this is the
//!   same simplicity trade-off as `crates/elephc-tls`.
//! - Fallible entry points collapse failure to a `-1` sentinel. String results
//!   return `*const c_char` pointing into a per-result static buffer that stays
//!   valid until the next call of the same function; elephc copies the bytes
//!   into an owned PHP string (`__rt_cstr_to_str`) immediately on return.
//! - SQLite is statically bundled (`libsqlite3-sys`'s `bundled` feature), so a
//!   compiled PHP binary that links this staticlib has no system SQLite runtime
//!   dependency. Strings cross the boundary NUL-terminated, so values with
//!   embedded NUL bytes (binary BLOBs) are not round-tripped through the text
//!   path — a documented MVP limitation.

use std::collections::HashMap;
use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_int};
use std::ptr;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Mutex, OnceLock};

use libsqlite3_sys as ffi;

/// A live SQLite connection handle. The raw pointer is `Send` in practice
/// because elephc programs drive one connection from one thread at a time; the
/// wrapper lets it live in the global table behind a mutex.
struct Conn(*mut ffi::sqlite3);
unsafe impl Send for Conn {}

/// A live prepared statement plus the connection pointer it belongs to (kept so
/// error messages can be read and owning connections matched without a second
/// table lookup).
struct Stmt {
    ptr: *mut ffi::sqlite3_stmt,
    db: *mut ffi::sqlite3,
}
unsafe impl Send for Stmt {}

/// Global connection table, keyed by the `i64` IDs handed back to the caller.
fn conns() -> &'static Mutex<HashMap<i64, Conn>> {
    static CONNS: OnceLock<Mutex<HashMap<i64, Conn>>> = OnceLock::new();
    CONNS.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Global prepared-statement table, keyed by the `i64` IDs handed back to the
/// caller.
fn stmts() -> &'static Mutex<HashMap<i64, Stmt>> {
    static STMTS: OnceLock<Mutex<HashMap<i64, Stmt>>> = OnceLock::new();
    STMTS.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Returns a fresh, never-reused handle ID. IDs start at 1 so `0` and `-1`
/// remain available as "absent" / "error" sentinels.
fn next_id() -> i64 {
    static NEXT: AtomicI64 = AtomicI64::new(1);
    NEXT.fetch_add(1, Ordering::SeqCst)
}

/// Static buffer holding the last message captured by a failed
/// `elephc_sqlite_open` (no connection exists yet to read `errmsg` from).
fn open_error_cell() -> &'static Mutex<CString> {
    static C: OnceLock<Mutex<CString>> = OnceLock::new();
    C.get_or_init(|| Mutex::new(CString::default()))
}

/// Static buffer for the most recent `elephc_sqlite_errmsg` result.
fn errmsg_cell() -> &'static Mutex<CString> {
    static C: OnceLock<Mutex<CString>> = OnceLock::new();
    C.get_or_init(|| Mutex::new(CString::default()))
}

/// Static buffer for the most recent `elephc_sqlite_column_name` result.
fn colname_cell() -> &'static Mutex<CString> {
    static C: OnceLock<Mutex<CString>> = OnceLock::new();
    C.get_or_init(|| Mutex::new(CString::default()))
}

/// Static buffer for the most recent `elephc_sqlite_column_text` result.
fn coltext_cell() -> &'static Mutex<CString> {
    static C: OnceLock<Mutex<CString>> = OnceLock::new();
    C.get_or_init(|| Mutex::new(CString::default()))
}

/// Stores `s` (with any embedded NUL bytes stripped) into the per-result static
/// `cell` and returns a pointer into that stored buffer. The pointer stays valid
/// until the next call that writes the same cell; elephc copies the bytes into an
/// owned PHP string on return, so single-threaded callers never observe a stale
/// pointer.
fn store_cstr(cell: &'static Mutex<CString>, s: &str) -> *const c_char {
    let bytes: Vec<u8> = s.bytes().filter(|&b| b != 0).collect();
    let cstr = CString::new(bytes).unwrap_or_default();
    let mut guard = cell.lock().unwrap();
    *guard = cstr;
    guard.as_ptr()
}

/// Reads a null-terminated C string argument (the shape elephc's `extern …`
/// string parameters marshal to) as a `&str`, returning `None` on a null pointer
/// or invalid UTF-8.
///
/// # Safety
///
/// `p`, when non-null, must point to a NUL-terminated string valid for the
/// duration of the call.
unsafe fn cstr_arg<'a>(p: *const c_char) -> Option<&'a str> {
    if p.is_null() {
        return None;
    }
    CStr::from_ptr(p).to_str().ok()
}

/// Reads SQLite's current error message for `db` into an owned `String`.
unsafe fn read_errmsg(db: *mut ffi::sqlite3) -> String {
    let p = ffi::sqlite3_errmsg(db);
    if p.is_null() {
        return String::new();
    }
    CStr::from_ptr(p).to_string_lossy().into_owned()
}

/// Returns the bridge ABI version. Bumped when the C ABI shape changes.
#[no_mangle]
pub extern "C" fn elephc_sqlite_version() -> i32 {
    1
}

/// Opens the SQLite database named by a PDO DSN (`sqlite:path`,
/// `sqlite::memory:`, or `sqlite:` for a private temp DB) and returns an `i64`
/// connection handle, or `-1` on failure (with the SQLite message stashed for
/// `elephc_sqlite_last_open_error`). A DSN without the `sqlite:` prefix is
/// rejected — the SQLite driver only claims `sqlite:` DSNs.
///
/// # Safety
///
/// `dsn` must point to a NUL-terminated string valid for the duration of the call.
#[no_mangle]
pub unsafe extern "C" fn elephc_sqlite_open(dsn: *const c_char) -> i64 {
    let Some(dsn) = cstr_arg(dsn) else {
        store_cstr(open_error_cell(), "invalid DSN");
        return -1;
    };
    let Some(path) = dsn.strip_prefix("sqlite:") else {
        store_cstr(
            open_error_cell(),
            "could not find driver (only sqlite: DSNs are supported)",
        );
        return -1;
    };
    let Ok(c_path) = CString::new(path) else {
        store_cstr(open_error_cell(), "invalid database path");
        return -1;
    };
    let mut db: *mut ffi::sqlite3 = ptr::null_mut();
    let flags = ffi::SQLITE_OPEN_READWRITE | ffi::SQLITE_OPEN_CREATE;
    let rc = ffi::sqlite3_open_v2(c_path.as_ptr(), &mut db, flags, ptr::null());
    if rc != ffi::SQLITE_OK {
        let msg = if db.is_null() {
            "unable to allocate database handle".to_string()
        } else {
            read_errmsg(db)
        };
        store_cstr(open_error_cell(), &msg);
        if !db.is_null() {
            ffi::sqlite3_close(db);
        }
        return -1;
    }
    let id = next_id();
    conns().lock().unwrap().insert(id, Conn(db));
    id
}

/// Returns a pointer to the message captured by the most recent failed
/// `elephc_sqlite_open`. Valid until the next failed open.
#[no_mangle]
pub extern "C" fn elephc_sqlite_last_open_error() -> *const c_char {
    open_error_cell().lock().unwrap().as_ptr()
}

/// Closes a connection (finalizing any statements still registered against it)
/// and removes it from the table. Unknown handles are ignored.
#[no_mangle]
pub extern "C" fn elephc_sqlite_close(conn_id: i64) {
    let db = conns().lock().unwrap().get(&conn_id).map(|c| c.0);
    if let Some(db) = db {
        // Finalize and drop any statements opened against this connection first,
        // so sqlite3_close does not fail with SQLITE_BUSY on leaked statements.
        let owned: Vec<i64> = stmts()
            .lock()
            .unwrap()
            .iter()
            .filter(|(_, s)| s.db == db)
            .map(|(k, _)| *k)
            .collect();
        for k in owned {
            if let Some(s) = stmts().lock().unwrap().remove(&k) {
                unsafe { ffi::sqlite3_finalize(s.ptr) };
            }
        }
        conns().lock().unwrap().remove(&conn_id);
        unsafe { ffi::sqlite3_close(db) };
    }
}

/// Runs one or more SQL statements with no result rows (`PDO::exec`). Returns
/// the number of rows changed by the statement, or `-1` on error.
///
/// # Safety
///
/// `sql` must point to a NUL-terminated string valid for the duration of the call.
#[no_mangle]
pub unsafe extern "C" fn elephc_sqlite_exec(conn_id: i64, sql: *const c_char) -> i64 {
    let guard = conns().lock().unwrap();
    let Some(conn) = guard.get(&conn_id) else {
        return -1;
    };
    if sql.is_null() {
        return -1;
    }
    let rc = ffi::sqlite3_exec(conn.0, sql, None, ptr::null_mut(), ptr::null_mut());
    if rc != ffi::SQLITE_OK {
        return -1;
    }
    ffi::sqlite3_changes(conn.0) as i64
}

/// Returns the rowid of the most recent successful INSERT on the connection.
#[no_mangle]
pub extern "C" fn elephc_sqlite_last_insert_id(conn_id: i64) -> i64 {
    let guard = conns().lock().unwrap();
    match guard.get(&conn_id) {
        Some(conn) => unsafe { ffi::sqlite3_last_insert_rowid(conn.0) },
        None => 0,
    }
}

/// Returns the number of rows changed by the most recent statement on the
/// connection (`PDOStatement::rowCount` for INSERT/UPDATE/DELETE).
#[no_mangle]
pub extern "C" fn elephc_sqlite_changes(conn_id: i64) -> i64 {
    let guard = conns().lock().unwrap();
    match guard.get(&conn_id) {
        Some(conn) => unsafe { ffi::sqlite3_changes(conn.0) as i64 },
        None => 0,
    }
}

/// Runs a single bare statement (used for BEGIN/COMMIT/ROLLBACK). Returns `1`
/// on success, `0` on failure — matching the PHP transaction-method bool.
unsafe fn exec_simple(conn_id: i64, sql: &[u8]) -> i64 {
    let guard = conns().lock().unwrap();
    let Some(conn) = guard.get(&conn_id) else {
        return 0;
    };
    let Ok(c_sql) = CString::new(sql) else {
        return 0;
    };
    let rc = ffi::sqlite3_exec(
        conn.0,
        c_sql.as_ptr(),
        None,
        ptr::null_mut(),
        ptr::null_mut(),
    );
    (rc == ffi::SQLITE_OK) as i64
}

/// Begins a deferred transaction (`PDO::beginTransaction`). Returns `1`/`0`.
#[no_mangle]
pub extern "C" fn elephc_sqlite_begin(conn_id: i64) -> i64 {
    unsafe { exec_simple(conn_id, b"BEGIN") }
}

/// Commits the active transaction (`PDO::commit`). Returns `1`/`0`.
#[no_mangle]
pub extern "C" fn elephc_sqlite_commit(conn_id: i64) -> i64 {
    unsafe { exec_simple(conn_id, b"COMMIT") }
}

/// Rolls back the active transaction (`PDO::rollBack`). Returns `1`/`0`.
#[no_mangle]
pub extern "C" fn elephc_sqlite_rollback(conn_id: i64) -> i64 {
    unsafe { exec_simple(conn_id, b"ROLLBACK") }
}

/// Returns SQLite's primary result code for the connection's last operation
/// (`PDO::errorCode` maps this to a SQLSTATE on the PHP side).
#[no_mangle]
pub extern "C" fn elephc_sqlite_errcode(conn_id: i64) -> i64 {
    let guard = conns().lock().unwrap();
    match guard.get(&conn_id) {
        Some(conn) => unsafe { ffi::sqlite3_errcode(conn.0) as i64 },
        None => -1,
    }
}

/// Returns a pointer to the connection's current error message
/// (`PDO::errorInfo` element 2). Valid until the next `elephc_sqlite_errmsg`.
#[no_mangle]
pub extern "C" fn elephc_sqlite_errmsg(conn_id: i64) -> *const c_char {
    let msg = {
        let guard = conns().lock().unwrap();
        match guard.get(&conn_id) {
            Some(conn) => unsafe { read_errmsg(conn.0) },
            None => String::new(),
        }
    };
    store_cstr(errmsg_cell(), &msg)
}

/// Prepares a statement on the connection (`PDO::prepare` / `PDO::query`) and
/// returns an `i64` statement handle, or `-1` on a compile error.
///
/// # Safety
///
/// `sql` must point to a NUL-terminated string valid for the duration of the call.
#[no_mangle]
pub unsafe extern "C" fn elephc_sqlite_prepare(conn_id: i64, sql: *const c_char) -> i64 {
    let guard = conns().lock().unwrap();
    let Some(conn) = guard.get(&conn_id) else {
        return -1;
    };
    if sql.is_null() {
        return -1;
    }
    let mut stmt: *mut ffi::sqlite3_stmt = ptr::null_mut();
    // -1 length lets SQLite read up to the NUL terminator.
    let rc = ffi::sqlite3_prepare_v2(conn.0, sql, -1, &mut stmt, ptr::null_mut());
    if rc != ffi::SQLITE_OK || stmt.is_null() {
        return -1;
    }
    let id = next_id();
    stmts().lock().unwrap().insert(
        id,
        Stmt {
            ptr: stmt,
            db: conn.0,
        },
    );
    id
}

/// Resolves a named placeholder to its 1-based bind index, trying the PDO/SQLite
/// prefixes (`:name`, `@name`, `$name`) and the bare name. Returns `0` when no
/// placeholder matches.
///
/// # Safety
///
/// `name` must point to a NUL-terminated string valid for the duration of the call.
#[no_mangle]
pub unsafe extern "C" fn elephc_sqlite_bind_parameter_index(
    stmt_id: i64,
    name: *const c_char,
) -> i64 {
    let guard = stmts().lock().unwrap();
    let Some(s) = guard.get(&stmt_id) else {
        return 0;
    };
    let Some(name) = cstr_arg(name) else {
        return 0;
    };
    let bare = name.strip_prefix(':').unwrap_or(name);
    for cand in [format!(":{bare}"), format!("@{bare}"), format!("${bare}")] {
        if let Ok(c) = CString::new(cand) {
            let idx = ffi::sqlite3_bind_parameter_index(s.ptr, c.as_ptr());
            if idx != 0 {
                return idx as i64;
            }
        }
    }
    0
}

/// Binds an integer to the 1-based placeholder `idx`. Returns `1`/`0`.
#[no_mangle]
pub extern "C" fn elephc_sqlite_bind_int(stmt_id: i64, idx: i64, val: i64) -> i64 {
    let guard = stmts().lock().unwrap();
    match guard.get(&stmt_id) {
        Some(s) => {
            let rc = unsafe { ffi::sqlite3_bind_int64(s.ptr, idx as c_int, val) };
            (rc == ffi::SQLITE_OK) as i64
        }
        None => 0,
    }
}

/// Binds a double to the 1-based placeholder `idx`. Returns `1`/`0`.
#[no_mangle]
pub extern "C" fn elephc_sqlite_bind_double(stmt_id: i64, idx: i64, val: f64) -> i64 {
    let guard = stmts().lock().unwrap();
    match guard.get(&stmt_id) {
        Some(s) => {
            let rc = unsafe { ffi::sqlite3_bind_double(s.ptr, idx as c_int, val) };
            (rc == ffi::SQLITE_OK) as i64
        }
        None => 0,
    }
}

/// Binds a text value (copied via `SQLITE_TRANSIENT`) to the 1-based placeholder
/// `idx`. Returns `1`/`0`.
///
/// # Safety
///
/// `val` must point to a NUL-terminated string valid for the duration of the call.
#[no_mangle]
pub unsafe extern "C" fn elephc_sqlite_bind_text(
    stmt_id: i64,
    idx: i64,
    val: *const c_char,
) -> i64 {
    let guard = stmts().lock().unwrap();
    let Some(s) = guard.get(&stmt_id) else {
        return 0;
    };
    if val.is_null() {
        return (unsafe { ffi::sqlite3_bind_null(s.ptr, idx as c_int) } == ffi::SQLITE_OK) as i64;
    }
    // -1 length lets SQLite read up to the NUL terminator; TRANSIENT copies it.
    let rc = ffi::sqlite3_bind_text(s.ptr, idx as c_int, val, -1, ffi::SQLITE_TRANSIENT());
    (rc == ffi::SQLITE_OK) as i64
}

/// Binds SQL NULL to the 1-based placeholder `idx`. Returns `1`/`0`.
#[no_mangle]
pub extern "C" fn elephc_sqlite_bind_null(stmt_id: i64, idx: i64) -> i64 {
    let guard = stmts().lock().unwrap();
    match guard.get(&stmt_id) {
        Some(s) => {
            let rc = unsafe { ffi::sqlite3_bind_null(s.ptr, idx as c_int) };
            (rc == ffi::SQLITE_OK) as i64
        }
        None => 0,
    }
}

/// Resets a statement so it can be re-executed, *keeping* its current parameter
/// bindings (so values set via `bindValue` / `bindParam` survive a no-argument
/// `PDOStatement::execute`). Returns `1`/`0`.
#[no_mangle]
pub extern "C" fn elephc_sqlite_reset(stmt_id: i64) -> i64 {
    let guard = stmts().lock().unwrap();
    match guard.get(&stmt_id) {
        Some(s) => {
            unsafe { ffi::sqlite3_reset(s.ptr) };
            1
        }
        None => 0,
    }
}

/// Clears all parameter bindings on a statement (`PDOStatement::execute` with a
/// fresh parameter array rebinds from scratch). Returns `1`/`0`.
#[no_mangle]
pub extern "C" fn elephc_sqlite_clear_bindings(stmt_id: i64) -> i64 {
    let guard = stmts().lock().unwrap();
    match guard.get(&stmt_id) {
        Some(s) => {
            unsafe { ffi::sqlite3_clear_bindings(s.ptr) };
            1
        }
        None => 0,
    }
}

/// Advances the statement one row. Returns `1` for a row, `0` when the result
/// set is exhausted, `-1` on error.
#[no_mangle]
pub extern "C" fn elephc_sqlite_step(stmt_id: i64) -> i64 {
    let guard = stmts().lock().unwrap();
    match guard.get(&stmt_id) {
        Some(s) => {
            let rc = unsafe { ffi::sqlite3_step(s.ptr) };
            match rc {
                ffi::SQLITE_ROW => 1,
                ffi::SQLITE_DONE => 0,
                _ => -1,
            }
        }
        None => -1,
    }
}

/// Returns the number of result columns for the statement.
#[no_mangle]
pub extern "C" fn elephc_sqlite_column_count(stmt_id: i64) -> i64 {
    let guard = stmts().lock().unwrap();
    match guard.get(&stmt_id) {
        Some(s) => unsafe { ffi::sqlite3_column_count(s.ptr) as i64 },
        None => 0,
    }
}

/// Returns a pointer to the name of result column `i` (0-based). Valid until the
/// next `elephc_sqlite_column_name`.
#[no_mangle]
pub extern "C" fn elephc_sqlite_column_name(stmt_id: i64, i: i64) -> *const c_char {
    let name = {
        let guard = stmts().lock().unwrap();
        match guard.get(&stmt_id) {
            Some(s) => unsafe {
                let p = ffi::sqlite3_column_name(s.ptr, i as c_int);
                if p.is_null() {
                    String::new()
                } else {
                    CStr::from_ptr(p).to_string_lossy().into_owned()
                }
            },
            None => String::new(),
        }
    };
    store_cstr(colname_cell(), &name)
}

/// Returns SQLite's type code for the current row's column `i` (0-based):
/// 1=INTEGER, 2=FLOAT, 3=TEXT, 4=BLOB, 5=NULL.
#[no_mangle]
pub extern "C" fn elephc_sqlite_column_type(stmt_id: i64, i: i64) -> i64 {
    let guard = stmts().lock().unwrap();
    match guard.get(&stmt_id) {
        Some(s) => unsafe { ffi::sqlite3_column_type(s.ptr, i as c_int) as i64 },
        None => ffi::SQLITE_NULL as i64,
    }
}

/// Returns the current row's column `i` (0-based) as an integer.
#[no_mangle]
pub extern "C" fn elephc_sqlite_column_int(stmt_id: i64, i: i64) -> i64 {
    let guard = stmts().lock().unwrap();
    match guard.get(&stmt_id) {
        Some(s) => unsafe { ffi::sqlite3_column_int64(s.ptr, i as c_int) },
        None => 0,
    }
}

/// Returns the current row's column `i` (0-based) as a double.
#[no_mangle]
pub extern "C" fn elephc_sqlite_column_double(stmt_id: i64, i: i64) -> f64 {
    let guard = stmts().lock().unwrap();
    match guard.get(&stmt_id) {
        Some(s) => unsafe { ffi::sqlite3_column_double(s.ptr, i as c_int) },
        None => 0.0,
    }
}

/// Returns a pointer to the current row's column `i` (0-based) text
/// representation. Valid until the next `elephc_sqlite_column_text`.
#[no_mangle]
pub extern "C" fn elephc_sqlite_column_text(stmt_id: i64, i: i64) -> *const c_char {
    let text = {
        let guard = stmts().lock().unwrap();
        match guard.get(&stmt_id) {
            Some(s) => unsafe {
                let p = ffi::sqlite3_column_text(s.ptr, i as c_int);
                if p.is_null() {
                    String::new()
                } else {
                    let n = ffi::sqlite3_column_bytes(s.ptr, i as c_int);
                    let bytes = std::slice::from_raw_parts(p as *const u8, n.max(0) as usize);
                    String::from_utf8_lossy(bytes).into_owned()
                }
            },
            None => String::new(),
        }
    };
    store_cstr(coltext_cell(), &text)
}

/// Finalizes a statement and removes it from the table. Unknown handles return
/// `0`; success returns `1`.
#[no_mangle]
pub extern "C" fn elephc_sqlite_finalize(stmt_id: i64) -> i64 {
    match stmts().lock().unwrap().remove(&stmt_id) {
        Some(s) => {
            unsafe { ffi::sqlite3_finalize(s.ptr) };
            1
        }
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

    /// The ABI version constant is the v1 surface described in this module.
    #[test]
    fn version_is_v1() {
        assert_eq!(elephc_sqlite_version(), 1);
    }

    /// A DSN without the `sqlite:` prefix is rejected and records a driver error.
    #[test]
    fn open_rejects_non_sqlite_dsn() {
        let dsn = cs("mysql:host=localhost");
        let id = unsafe { elephc_sqlite_open(dsn.as_ptr()) };
        assert_eq!(id, -1);
        let msg = unsafe { read(elephc_sqlite_last_open_error()) };
        assert!(msg.contains("driver"), "got: {msg}");
    }

    /// Unknown handles return the documented sentinels rather than panicking.
    #[test]
    fn unknown_handles_return_sentinels() {
        assert_eq!(elephc_sqlite_step(999_999), -1);
        assert_eq!(elephc_sqlite_column_count(999_999), 0);
        assert_eq!(elephc_sqlite_finalize(999_999), 0);
        let sql = cs("SELECT 1");
        unsafe { elephc_sqlite_exec(999_999, sql.as_ptr()) };
    }

    /// Full in-memory round-trip: open, create, insert, prepared select with a
    /// positional bind, step, and read typed columns back.
    #[test]
    fn in_memory_round_trip() {
        let dsn = cs("sqlite::memory:");
        let conn = unsafe { elephc_sqlite_open(dsn.as_ptr()) };
        assert!(conn > 0, "open failed");

        let ddl = cs("CREATE TABLE users (id INTEGER PRIMARY KEY, name TEXT, score REAL)");
        assert_eq!(unsafe { elephc_sqlite_exec(conn, ddl.as_ptr()) }, 0);

        let ins = cs("INSERT INTO users (name, score) VALUES ('Alice', 9.5)");
        assert_eq!(unsafe { elephc_sqlite_exec(conn, ins.as_ptr()) }, 1);
        assert_eq!(elephc_sqlite_last_insert_id(conn), 1);

        let ins2 = cs("INSERT INTO users (name, score) VALUES ('Bob', 7.0)");
        assert_eq!(unsafe { elephc_sqlite_exec(conn, ins2.as_ptr()) }, 1);

        let sql = cs("SELECT id, name, score FROM users WHERE id = ?");
        let stmt = unsafe { elephc_sqlite_prepare(conn, sql.as_ptr()) };
        assert!(stmt > 0, "prepare failed");
        assert_eq!(elephc_sqlite_bind_int(stmt, 1, 1), 1);

        assert_eq!(elephc_sqlite_step(stmt), 1);
        assert_eq!(elephc_sqlite_column_count(stmt), 3);
        assert_eq!(elephc_sqlite_column_int(stmt, 0), 1);
        let name = unsafe { read(elephc_sqlite_column_name(stmt, 1)) };
        assert_eq!(name, "name");
        let val = unsafe { read(elephc_sqlite_column_text(stmt, 1)) };
        assert_eq!(val, "Alice");
        assert_eq!(elephc_sqlite_column_double(stmt, 2), 9.5);
        // No more rows for id = 1.
        assert_eq!(elephc_sqlite_step(stmt), 0);

        assert_eq!(elephc_sqlite_finalize(stmt), 1);
        elephc_sqlite_close(conn);
    }

    /// Named placeholders resolve through `bind_parameter_index` with and
    /// without the leading colon, and a bad SQL prepare returns `-1`.
    #[test]
    fn named_binds_and_prepare_error() {
        let dsn = cs("sqlite::memory:");
        let conn = unsafe { elephc_sqlite_open(dsn.as_ptr()) };
        assert!(conn > 0);
        let ddl = cs("CREATE TABLE t (a INTEGER)");
        unsafe { elephc_sqlite_exec(conn, ddl.as_ptr()) };

        let sql = cs("INSERT INTO t (a) VALUES (:val)");
        let stmt = unsafe { elephc_sqlite_prepare(conn, sql.as_ptr()) };
        assert!(stmt > 0);
        let colon = cs(":val");
        let bare = cs("val");
        let idx_colon = unsafe { elephc_sqlite_bind_parameter_index(stmt, colon.as_ptr()) };
        let idx_bare = unsafe { elephc_sqlite_bind_parameter_index(stmt, bare.as_ptr()) };
        assert_eq!(idx_colon, 1);
        assert_eq!(idx_bare, 1);
        assert_eq!(elephc_sqlite_bind_int(stmt, idx_bare, 42), 1);
        assert_eq!(elephc_sqlite_step(stmt), 0);
        elephc_sqlite_finalize(stmt);

        let bad = cs("SELECT FROM WHERE bogus");
        assert_eq!(unsafe { elephc_sqlite_prepare(conn, bad.as_ptr()) }, -1);

        elephc_sqlite_close(conn);
    }

    /// A positional bind (slot 1) survives a `bind_parameter_index` lookup for a
    /// sibling named parameter — i.e. mixing `?` and `:name` in one statement
    /// binds both. (Isolates the bridge from compiler codegen.)
    #[test]
    fn mixed_positional_and_named_bind() {
        let dsn = cs("sqlite::memory:");
        let conn = unsafe { elephc_sqlite_open(dsn.as_ptr()) };
        let ddl = cs("CREATE TABLE t (id INTEGER, name TEXT)");
        unsafe { elephc_sqlite_exec(conn, ddl.as_ptr()) };

        let ins = cs("INSERT INTO t (id, name) VALUES (?, :name)");
        let stmt = unsafe { elephc_sqlite_prepare(conn, ins.as_ptr()) };
        assert_eq!(elephc_sqlite_bind_int(stmt, 1, 10), 1);
        let nm = cs(":name");
        let idx = unsafe { elephc_sqlite_bind_parameter_index(stmt, nm.as_ptr()) };
        assert_eq!(idx, 2);
        let ada = cs("Ada");
        assert_eq!(unsafe { elephc_sqlite_bind_text(stmt, idx, ada.as_ptr()) }, 1);
        assert_eq!(elephc_sqlite_step(stmt), 0);
        elephc_sqlite_finalize(stmt);

        let sel = cs("SELECT id, name FROM t");
        let q = unsafe { elephc_sqlite_prepare(conn, sel.as_ptr()) };
        assert_eq!(elephc_sqlite_step(q), 1);
        assert_eq!(elephc_sqlite_column_int(q, 0), 10);
        assert_eq!(unsafe { read(elephc_sqlite_column_text(q, 1)) }, "Ada");
        elephc_sqlite_finalize(q);
        elephc_sqlite_close(conn);
    }

    /// `reset` keeps parameter bindings (so a re-step reuses them), while
    /// `clear_bindings` drops them (a later step binds SQL NULL).
    #[test]
    fn reset_keeps_bindings_clear_removes() {
        let dsn = cs("sqlite::memory:");
        let conn = unsafe { elephc_sqlite_open(dsn.as_ptr()) };
        let ddl = cs("CREATE TABLE t (a INTEGER)");
        unsafe { elephc_sqlite_exec(conn, ddl.as_ptr()) };

        let ins = cs("INSERT INTO t (a) VALUES (?)");
        let stmt = unsafe { elephc_sqlite_prepare(conn, ins.as_ptr()) };
        assert_eq!(elephc_sqlite_bind_int(stmt, 1, 5), 1);
        assert_eq!(elephc_sqlite_step(stmt), 0);
        // reset keeps the binding, so the second insert is also a = 5.
        assert_eq!(elephc_sqlite_reset(stmt), 1);
        assert_eq!(elephc_sqlite_step(stmt), 0);
        elephc_sqlite_finalize(stmt);

        let count_fives = cs("SELECT COUNT(*) FROM t WHERE a = 5");
        let q = unsafe { elephc_sqlite_prepare(conn, count_fives.as_ptr()) };
        assert_eq!(elephc_sqlite_step(q), 1);
        assert_eq!(elephc_sqlite_column_int(q, 0), 2);
        elephc_sqlite_finalize(q);

        // clear_bindings drops the binding, so the next insert stores NULL.
        let stmt2 = unsafe { elephc_sqlite_prepare(conn, ins.as_ptr()) };
        assert_eq!(elephc_sqlite_bind_int(stmt2, 1, 9), 1);
        assert_eq!(elephc_sqlite_clear_bindings(stmt2), 1);
        assert_eq!(elephc_sqlite_step(stmt2), 0);
        elephc_sqlite_finalize(stmt2);

        let count_nulls = cs("SELECT COUNT(*) FROM t WHERE a IS NULL");
        let q2 = unsafe { elephc_sqlite_prepare(conn, count_nulls.as_ptr()) };
        assert_eq!(elephc_sqlite_step(q2), 1);
        assert_eq!(elephc_sqlite_column_int(q2, 0), 1);
        elephc_sqlite_finalize(q2);

        elephc_sqlite_close(conn);
    }
}
