//! Purpose:
//! The SQLite driver for the elephc PDO bridge. Wraps the bundled SQLite C
//! library behind a small set of methods that the driver-agnostic C ABI in
//! `lib.rs` dispatches to for `sqlite:` connections.
//!
//! Called from:
//! - `crate::lib`'s `elephc_pdo_*` C-ABI functions, after matching the
//!   connection/statement's driver to `Conn::Sqlite` / `Stmt::Sqlite`.
//!
//! Key details:
//! - SQLite is statically bundled (`libsqlite3-sys`'s `bundled` feature), so a
//!   compiled PHP binary that links this staticlib has no system SQLite runtime
//!   dependency.
//! - Column type codes match SQLite's: 1=INTEGER, 2=FLOAT, 3=TEXT, 4=BLOB,
//!   5=NULL — the same codes the PDO prelude's `columnValue()` reads.

use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_int};
use std::ptr;

use libsqlite3_sys as ffi;

/// A live SQLite connection. The raw pointer is `Send` in practice because
/// elephc programs drive one connection from one thread at a time.
pub struct SqliteConn {
    pub db: *mut ffi::sqlite3,
}
unsafe impl Send for SqliteConn {}

/// A live SQLite prepared statement plus the connection pointer it belongs to.
pub struct SqliteStmt {
    pub ptr: *mut ffi::sqlite3_stmt,
    pub db: *mut ffi::sqlite3,
}
unsafe impl Send for SqliteStmt {}

/// Reads SQLite's current error message for a connection into an owned `String`.
unsafe fn read_errmsg(db: *mut ffi::sqlite3) -> String {
    let p = ffi::sqlite3_errmsg(db);
    if p.is_null() {
        return String::new();
    }
    CStr::from_ptr(p).to_string_lossy().into_owned()
}

impl SqliteConn {
    /// Opens the SQLite database at `path` (the DSN body after `sqlite:`),
    /// returning the connection or an error message.
    pub fn open(path: &str) -> Result<SqliteConn, String> {
        let Ok(c_path) = CString::new(path) else {
            return Err("invalid database path".to_string());
        };
        let mut db: *mut ffi::sqlite3 = ptr::null_mut();
        let flags = ffi::SQLITE_OPEN_READWRITE | ffi::SQLITE_OPEN_CREATE;
        let rc = unsafe { ffi::sqlite3_open_v2(c_path.as_ptr(), &mut db, flags, ptr::null()) };
        if rc != ffi::SQLITE_OK {
            let msg = if db.is_null() {
                "unable to allocate database handle".to_string()
            } else {
                unsafe { read_errmsg(db) }
            };
            if !db.is_null() {
                unsafe { ffi::sqlite3_close(db) };
            }
            return Err(msg);
        }
        Ok(SqliteConn { db })
    }

    /// Runs one or more statements with no result rows (`PDO::exec`). Returns the
    /// number of rows changed, or `-1` on error.
    ///
    /// # Safety
    /// `sql` must point to a NUL-terminated string valid for the call.
    pub unsafe fn exec(&self, sql: *const c_char) -> i64 {
        if sql.is_null() {
            return -1;
        }
        let rc = ffi::sqlite3_exec(self.db, sql, None, ptr::null_mut(), ptr::null_mut());
        if rc != ffi::SQLITE_OK {
            return -1;
        }
        ffi::sqlite3_changes(self.db) as i64
    }

    /// Returns the rowid of the most recent successful INSERT.
    pub fn last_insert_id(&self) -> i64 {
        unsafe { ffi::sqlite3_last_insert_rowid(self.db) }
    }

    /// Returns the number of rows changed by the most recent statement.
    pub fn changes(&self) -> i64 {
        unsafe { ffi::sqlite3_changes(self.db) as i64 }
    }

    /// Runs a single bare statement (BEGIN/COMMIT/ROLLBACK), returning `1`/`0`.
    pub fn exec_simple(&self, sql: &[u8]) -> i64 {
        let Ok(c_sql) = CString::new(sql) else {
            return 0;
        };
        let rc = unsafe {
            ffi::sqlite3_exec(self.db, c_sql.as_ptr(), None, ptr::null_mut(), ptr::null_mut())
        };
        (rc == ffi::SQLITE_OK) as i64
    }

    /// Returns SQLite's primary result code for the connection's last operation.
    pub fn errcode(&self) -> i64 {
        unsafe { ffi::sqlite3_errcode(self.db) as i64 }
    }

    /// Returns the connection's current error message.
    pub fn errmsg(&self) -> String {
        unsafe { read_errmsg(self.db) }
    }

    /// Prepares a statement, returning the statement handle or `()` on error.
    ///
    /// # Safety
    /// `sql` must point to a NUL-terminated string valid for the call.
    pub unsafe fn prepare(&self, sql: *const c_char) -> Result<SqliteStmt, ()> {
        if sql.is_null() {
            return Err(());
        }
        let mut stmt: *mut ffi::sqlite3_stmt = ptr::null_mut();
        // -1 length lets SQLite read up to the NUL terminator.
        let rc = ffi::sqlite3_prepare_v2(self.db, sql, -1, &mut stmt, ptr::null_mut());
        if rc != ffi::SQLITE_OK || stmt.is_null() {
            return Err(());
        }
        Ok(SqliteStmt {
            ptr: stmt,
            db: self.db,
        })
    }

    /// Closes the connection (the caller finalizes its statements first).
    pub fn close(&self) {
        unsafe { ffi::sqlite3_close(self.db) };
    }
}

impl SqliteStmt {
    /// Resolves a named placeholder to its 1-based bind index, trying the
    /// `:name`, `@name`, `$name` prefixes and the bare name. Returns `0` when no
    /// placeholder matches.
    pub fn bind_parameter_index(&self, name: &str) -> i64 {
        let bare = name.strip_prefix(':').unwrap_or(name);
        for cand in [format!(":{bare}"), format!("@{bare}"), format!("${bare}")] {
            if let Ok(c) = CString::new(cand) {
                let idx = unsafe { ffi::sqlite3_bind_parameter_index(self.ptr, c.as_ptr()) };
                if idx != 0 {
                    return idx as i64;
                }
            }
        }
        0
    }

    /// Binds an integer to the 1-based placeholder `idx`. Returns `1`/`0`.
    pub fn bind_int(&self, idx: i64, val: i64) -> i64 {
        let rc = unsafe { ffi::sqlite3_bind_int64(self.ptr, idx as c_int, val) };
        (rc == ffi::SQLITE_OK) as i64
    }

    /// Binds a double to the 1-based placeholder `idx`. Returns `1`/`0`.
    pub fn bind_double(&self, idx: i64, val: f64) -> i64 {
        let rc = unsafe { ffi::sqlite3_bind_double(self.ptr, idx as c_int, val) };
        (rc == ffi::SQLITE_OK) as i64
    }

    /// Binds a text value (copied via `SQLITE_TRANSIENT`) to placeholder `idx`.
    /// A null pointer binds SQL NULL. Returns `1`/`0`.
    ///
    /// # Safety
    /// `val`, when non-null, must point to a NUL-terminated string valid for the call.
    pub unsafe fn bind_text(&self, idx: i64, val: *const c_char) -> i64 {
        if val.is_null() {
            return (ffi::sqlite3_bind_null(self.ptr, idx as c_int) == ffi::SQLITE_OK) as i64;
        }
        let rc = ffi::sqlite3_bind_text(self.ptr, idx as c_int, val, -1, ffi::SQLITE_TRANSIENT());
        (rc == ffi::SQLITE_OK) as i64
    }

    /// Binds SQL NULL to the 1-based placeholder `idx`. Returns `1`/`0`.
    pub fn bind_null(&self, idx: i64) -> i64 {
        let rc = unsafe { ffi::sqlite3_bind_null(self.ptr, idx as c_int) };
        (rc == ffi::SQLITE_OK) as i64
    }

    /// Resets the statement, keeping its parameter bindings. Returns `1`.
    pub fn reset(&self) -> i64 {
        unsafe { ffi::sqlite3_reset(self.ptr) };
        1
    }

    /// Clears all parameter bindings on the statement. Returns `1`.
    pub fn clear_bindings(&self) -> i64 {
        unsafe { ffi::sqlite3_clear_bindings(self.ptr) };
        1
    }

    /// Advances the statement one row: `1` for a row, `0` when exhausted, `-1`
    /// on error.
    pub fn step(&self) -> i64 {
        let rc = unsafe { ffi::sqlite3_step(self.ptr) };
        match rc {
            ffi::SQLITE_ROW => 1,
            ffi::SQLITE_DONE => 0,
            _ => -1,
        }
    }

    /// Returns the number of result columns for the statement.
    pub fn column_count(&self) -> i64 {
        unsafe { ffi::sqlite3_column_count(self.ptr) as i64 }
    }

    /// Returns the name of result column `i` (0-based).
    pub fn column_name(&self, i: i64) -> String {
        unsafe {
            let p = ffi::sqlite3_column_name(self.ptr, i as c_int);
            if p.is_null() {
                String::new()
            } else {
                CStr::from_ptr(p).to_string_lossy().into_owned()
            }
        }
    }

    /// Returns SQLite's type code for the current row's column `i` (0-based):
    /// 1=INTEGER, 2=FLOAT, 3=TEXT, 4=BLOB, 5=NULL.
    pub fn column_type(&self, i: i64) -> i64 {
        unsafe { ffi::sqlite3_column_type(self.ptr, i as c_int) as i64 }
    }

    /// Returns the current row's column `i` (0-based) as an integer.
    pub fn column_int(&self, i: i64) -> i64 {
        unsafe { ffi::sqlite3_column_int64(self.ptr, i as c_int) }
    }

    /// Returns the current row's column `i` (0-based) as a double.
    pub fn column_double(&self, i: i64) -> f64 {
        unsafe { ffi::sqlite3_column_double(self.ptr, i as c_int) }
    }

    /// Returns the current row's column `i` (0-based) text representation.
    pub fn column_text(&self, i: i64) -> String {
        unsafe {
            let p = ffi::sqlite3_column_text(self.ptr, i as c_int);
            if p.is_null() {
                String::new()
            } else {
                let n = ffi::sqlite3_column_bytes(self.ptr, i as c_int);
                let bytes = std::slice::from_raw_parts(p as *const u8, n.max(0) as usize);
                String::from_utf8_lossy(bytes).into_owned()
            }
        }
    }

    /// Finalizes the statement.
    pub fn finalize(&self) {
        unsafe { ffi::sqlite3_finalize(self.ptr) };
    }
}
