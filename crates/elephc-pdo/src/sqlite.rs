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
use std::os::raw::{c_char, c_int, c_void};
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

/// Maps a SQLite primary result code to its 5-char SQLSTATE, mirroring PHP's
/// `pdo_sqlite` driver (`ext/pdo_sqlite/sqlite_driver.c`, `pdo_sqlite_error`):
/// `SQLITE_NOTFOUND`/`SQLITE_INTERRUPT`/`SQLITE_NOLFS`/`SQLITE_TOOBIG`/
/// `SQLITE_CONSTRAINT` get their own SQLSTATE, everything else (including
/// `SQLITE_ERROR`, `SQLITE_BUSY`, `SQLITE_LOCKED`, and the permission/read-only
/// family) falls back to the driver's generic `HY000`. `SQLITE_OK` is not part of
/// that error-only table — it is the bridge's own "no error" default, added here
/// so the mapping is total over every primary result code SQLite can report.
pub fn sqlite_sqlstate(rc: c_int) -> &'static str {
    match rc {
        ffi::SQLITE_OK => "00000",
        ffi::SQLITE_NOTFOUND => "42S02",
        ffi::SQLITE_INTERRUPT => "01002",
        ffi::SQLITE_NOLFS => "IM001",
        ffi::SQLITE_TOOBIG => "22001",
        ffi::SQLITE_CONSTRAINT => "23000",
        _ => "HY000",
    }
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
            ffi::sqlite3_exec(
                self.db,
                c_sql.as_ptr(),
                None,
                ptr::null_mut(),
                ptr::null_mut(),
            )
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

    /// Returns the 5-char SQLSTATE for the connection's last operation.
    pub fn sqlstate(&self) -> String {
        sqlite_sqlstate(unsafe { ffi::sqlite3_errcode(self.db) }).to_string()
    }

    /// Sets the number of milliseconds SQLite retries a locked database before
    /// giving up with `SQLITE_BUSY` (`sqlite3_busy_timeout`). Returns `1`/`0`.
    pub fn set_busy_timeout(&self, ms: i64) -> i64 {
        let rc = unsafe { ffi::sqlite3_busy_timeout(self.db, ms as c_int) };
        (rc == ffi::SQLITE_OK) as i64
    }

    /// Returns the bundled SQLite library's version string (e.g. `"3.46.0"`).
    pub fn server_version(&self) -> String {
        unsafe {
            let p = ffi::sqlite3_libversion();
            if p.is_null() {
                String::new()
            } else {
                CStr::from_ptr(p).to_string_lossy().into_owned()
            }
        }
    }

    /// Loads the SQLite extension at `path` (its entry point auto-derived, as PHP's
    /// `Pdo\Sqlite::loadExtension()` does), returning 1 on success or 0 on error.
    /// Extension loading is enabled only for the duration of the call and disabled
    /// again afterward to keep the default hardened posture. The freed error message
    /// is discarded (the caller reports failure via the connection's error state /
    /// a thrown exception).
    ///
    /// # Safety
    /// Loading an extension executes arbitrary native code from `path`; the caller
    /// is trusted to supply a library it intends to run.
    pub fn load_extension(&self, path: &str) -> i64 {
        let Ok(c_path) = CString::new(path) else {
            return 0;
        };
        unsafe {
            ffi::sqlite3_enable_load_extension(self.db, 1);
            let mut errmsg: *mut c_char = ptr::null_mut();
            let rc =
                ffi::sqlite3_load_extension(self.db, c_path.as_ptr(), ptr::null(), &mut errmsg);
            ffi::sqlite3_enable_load_extension(self.db, 0);
            if !errmsg.is_null() {
                ffi::sqlite3_free(errmsg as *mut _);
            }
            (rc == ffi::SQLITE_OK) as i64
        }
    }

    /// Reads a BLOB cell whole through the incremental-blob API
    /// (`sqlite3_blob_open` read-only, `sqlite3_blob_bytes`, `sqlite3_blob_read`),
    /// returning its raw bytes. `dbname` selects the attached database ("main" by
    /// default), `rowid` is the row's integer key, and `column` names the BLOB
    /// column. A missing row/column, or a column that cannot be opened as a blob,
    /// surfaces as `Err(message)`. Backs `Pdo\Sqlite::openBlob()` (read-whole).
    pub fn blob_read(
        &self,
        dbname: &str,
        table: &str,
        column: &str,
        rowid: i64,
    ) -> Result<Vec<u8>, String> {
        let c_db = CString::new(dbname).map_err(|_| "invalid database name".to_string())?;
        let c_table = CString::new(table).map_err(|_| "invalid table name".to_string())?;
        let c_col = CString::new(column).map_err(|_| "invalid column name".to_string())?;
        unsafe {
            let mut blob: *mut ffi::sqlite3_blob = ptr::null_mut();
            // flags = 0 opens the blob read-only, which is all read-whole needs.
            let rc = ffi::sqlite3_blob_open(
                self.db,
                c_db.as_ptr(),
                c_table.as_ptr(),
                c_col.as_ptr(),
                rowid,
                0,
                &mut blob,
            );
            if rc != ffi::SQLITE_OK || blob.is_null() {
                return Err(read_errmsg(self.db));
            }
            let n = ffi::sqlite3_blob_bytes(blob);
            let mut buf = vec![0u8; n.max(0) as usize];
            let read_rc = if n > 0 {
                ffi::sqlite3_blob_read(blob, buf.as_mut_ptr() as *mut c_void, n, 0)
            } else {
                ffi::SQLITE_OK
            };
            // Capture the error text before closing, since close resets the handle.
            let err = (read_rc != ffi::SQLITE_OK).then(|| read_errmsg(self.db));
            ffi::sqlite3_blob_close(blob);
            match err {
                Some(msg) => Err(msg),
                None => Ok(buf),
            }
        }
    }

    /// Registers a custom collation `name` backed by a compiled-PHP comparator
    /// (`Pdo\Sqlite::createCollation`). `descriptor` is the callable's 64-byte
    /// descriptor pointer and `adapter` the address of the codegen collation
    /// adapter (`__rt_pdo_call_collation`); both are threaded to the `x_compare`
    /// dispatcher through SQLite's per-registration `pApp`, so any number of
    /// collations coexist on one connection. Returns `1` on success, `0` on error.
    ///
    /// # Safety
    /// `descriptor`/`adapter` must be the live callable descriptor and adapter
    /// entry of the calling compiled program; both are kept alive by the PDO
    /// object rooting the callable, so the bridge stores them without touching the
    /// descriptor's (arena-managed) refcount.
    pub unsafe fn create_collation(
        &self,
        name: &str,
        descriptor: *mut c_void,
        adapter: *const c_void,
    ) -> i64 {
        let Ok(c_name) = CString::new(name) else {
            return 0;
        };
        let reg = Box::into_raw(Box::new(UdfReg { descriptor, adapter })) as *mut c_void;
        // `_v2` invokes `x_destroy` (freeing the box) even when it returns an
        // error, so the success path is the only one that must not free here.
        let rc = ffi::sqlite3_create_collation_v2(
            self.db,
            c_name.as_ptr(),
            ffi::SQLITE_UTF8,
            reg,
            Some(x_compare),
            Some(x_destroy),
        );
        if rc == ffi::SQLITE_OK {
            1
        } else {
            0
        }
    }
}

/// The C-ABI adapter that re-enters a compiled-PHP collation comparator. Emitted
/// by codegen as `__rt_pdo_call_collation`; the bridge only stores and calls its
/// address. It boxes the two byte buffers as PHP strings, invokes the callable
/// descriptor's uniform invoker, and returns the comparison sign clamped to
/// -1/0/1 (or a sentinel the dispatcher maps to "equal" when the comparator threw).
type CollationAdapter = unsafe extern "C" fn(
    descriptor: *mut c_void,
    a: *const u8,
    a_len: i64,
    b: *const u8,
    b_len: i64,
) -> i64;

/// A registered SQLite user callback. Boxed and handed to SQLite as the
/// registration's `pApp`, recovered in the dispatcher, and freed by `x_destroy`
/// at `sqlite3_close` / re-registration. The compiled-PHP callable `descriptor`
/// is kept alive by the PDO object rooting the callable (`$this->udfCallbacks`),
/// so the bridge holds it as a bare pointer and never touches its refcount (which
/// lives in the compiled program's arena, unreachable from this staticlib).
struct UdfReg {
    /// The 64-byte compiled-PHP callable descriptor pointer.
    descriptor: *mut c_void,
    /// The shared codegen adapter entry that re-enters the descriptor.
    adapter: *const c_void,
}

/// SQLite collation dispatcher (`xCompare`). Recovers the `UdfReg` from `pApp`
/// and re-enters the compiled-PHP comparator through its codegen adapter, passing
/// the two byte buffers SQLite provides (not NUL-terminated — the adapter consumes
/// explicit lengths). Returns the comparison sign in -1/0/1.
///
/// # Safety
/// `p_arg` is the `pApp` from registration (a live `Box<UdfReg>` pointer); `a`/`b`
/// point to `n_a`/`n_b` bytes valid for the call.
unsafe extern "C" fn x_compare(
    p_arg: *mut c_void,
    n_a: c_int,
    a: *const c_void,
    n_b: c_int,
    b: *const c_void,
) -> c_int {
    if p_arg.is_null() {
        return 0;
    }
    let reg = &*(p_arg as *const UdfReg);
    let adapter: CollationAdapter = std::mem::transmute(reg.adapter);
    let sign = adapter(
        reg.descriptor,
        a as *const u8,
        n_a as i64,
        b as *const u8,
        n_b as i64,
    );
    sign.clamp(-1, 1) as c_int
}

/// Frees a `Box<UdfReg>` when SQLite deletes a registration (connection close or
/// re-registration under the same name). Registered as every callback's `xDestroy`.
///
/// # Safety
/// `p_arg` must be a pointer produced by `Box::into_raw` for a `UdfReg`.
unsafe extern "C" fn x_destroy(p_arg: *mut c_void) {
    if !p_arg.is_null() {
        drop(Box::from_raw(p_arg as *mut UdfReg));
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

    /// Binds raw bytes (copied via `SQLITE_TRANSIENT`) to placeholder `idx`,
    /// preserving embedded NUL bytes that `bind_text`'s NUL-terminated string
    /// path cannot. A null pointer binds SQL NULL. A non-positive or
    /// `c_int`-overflowing `len` is treated as a zero-length blob rather than
    /// being cast as-is, which would silently wrap/truncate when handed to
    /// `sqlite3_bind_blob`'s `c_int` length parameter. Returns `1`/`0`.
    ///
    /// # Safety
    /// `ptr`, when non-null, must point to at least `len` readable bytes valid for
    /// the call.
    pub unsafe fn bind_blob(&self, idx: i64, ptr: *const c_char, len: i64) -> i64 {
        if ptr.is_null() {
            return (ffi::sqlite3_bind_null(self.ptr, idx as c_int) == ffi::SQLITE_OK) as i64;
        }
        let safe_len = if len <= 0 || len > c_int::MAX as i64 {
            0
        } else {
            len as c_int
        };
        let rc = ffi::sqlite3_bind_blob(
            self.ptr,
            idx as c_int,
            ptr as *const std::os::raw::c_void,
            safe_len,
            ffi::SQLITE_TRANSIENT(),
        );
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

    /// Returns the declared type of result column `i` (`sqlite3_column_decltype`),
    /// e.g. "INTEGER" or "TEXT", or an empty string for an expression column with no
    /// declared type. Feeds `PDOStatement::getColumnMeta`'s native_type.
    pub fn column_decltype(&self, i: i64) -> String {
        unsafe {
            let p = ffi::sqlite3_column_decltype(self.ptr, i as c_int);
            if p.is_null() {
                return String::new();
            }
            CStr::from_ptr(p).to_string_lossy().into_owned()
        }
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
        String::from_utf8_lossy(&self.column_data(i)).into_owned()
    }

    /// Returns the current row's column `i` (0-based) as raw SQLite bytes.
    /// This uses SQLite's byte-counted column API, so embedded NUL bytes are
    /// preserved for BLOBs and text values alike.
    pub fn column_data(&self, i: i64) -> Vec<u8> {
        unsafe {
            let p = ffi::sqlite3_column_blob(self.ptr, i as c_int);
            if p.is_null() {
                Vec::new()
            } else {
                let n = ffi::sqlite3_column_bytes(self.ptr, i as c_int);
                let bytes = std::slice::from_raw_parts(p as *const u8, n.max(0) as usize);
                bytes.to_vec()
            }
        }
    }

    /// Finalizes the statement.
    pub fn finalize(&self) {
        unsafe { ffi::sqlite3_finalize(self.ptr) };
    }

    /// Returns SQLite's primary result code for the statement's connection's last
    /// operation (SQLite tracks error state per-connection, not per-statement).
    pub fn errcode(&self) -> i64 {
        unsafe { ffi::sqlite3_errcode(self.db) as i64 }
    }

    /// Returns the statement's connection's current error message (see `errcode`).
    pub fn errmsg(&self) -> String {
        unsafe { read_errmsg(self.db) }
    }

    /// Returns the 5-char SQLSTATE for the statement's last operation.
    pub fn sqlstate(&self) -> String {
        sqlite_sqlstate(unsafe { ffi::sqlite3_errcode(self.db) }).to_string()
    }
}
