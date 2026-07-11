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
    ///
    /// `open_flags` is the raw `sqlite3_open_v2` flags to use, taken from
    /// `Pdo\Sqlite::ATTR_OPEN_FLAGS` (P1-10); `0` means "no override", which
    /// keeps the default `READWRITE|CREATE` PHP uses when the option is not
    /// set. `Pdo\Sqlite::OPEN_READONLY`/`OPEN_READWRITE`/`OPEN_CREATE` share
    /// their bit values with `SQLITE_OPEN_READONLY`/`_READWRITE`/`_CREATE`, so
    /// the PHP-side int crosses unchanged. When `path` starts with `file:`
    /// (P2-9's URI DSN, e.g. `sqlite:file:test.db?mode=ro`), `SQLITE_OPEN_URI`
    /// is OR-ed in regardless of `open_flags` so the query-string is honored.
    pub fn open(path: &str, open_flags: i64) -> Result<SqliteConn, String> {
        let Ok(c_path) = CString::new(path) else {
            return Err("invalid database path".to_string());
        };
        let mut db: *mut ffi::sqlite3 = ptr::null_mut();
        let mut flags: c_int = if open_flags != 0 {
            open_flags as c_int
        } else {
            ffi::SQLITE_OPEN_READWRITE | ffi::SQLITE_OPEN_CREATE
        };
        if path.starts_with("file:") {
            flags |= ffi::SQLITE_OPEN_URI;
        }
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
        // P2-7: PHP's pdo_sqlite seeds a 60s busy-timeout at connect time so a
        // lock contention (another connection mid-write) retries instead of
        // failing immediately with SQLITE_BUSY. `ATTR_TIMEOUT`/`setAttribute`
        // still override this later via `set_busy_timeout`.
        unsafe { ffi::sqlite3_busy_timeout(db, 60_000) };
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

    /// Registers a scalar SQL function `name` backed by a compiled-PHP callable
    /// (`Pdo\Sqlite::createFunction`). `num_args` is the declared arity (-1 =
    /// variadic), `flags` an optional `SQLITE_DETERMINISTIC` OR-ed into the text
    /// encoding, and `descriptor`/`adapter` the callable descriptor pointer and the
    /// codegen scalar adapter (`__rt_pdo_call_scalar`) threaded to `x_scalar` through
    /// SQLite's per-registration `pApp`. Returns `1` on success, `0` on error.
    ///
    /// # Safety
    /// `descriptor`/`adapter` must be the live callable descriptor and adapter entry
    /// of the calling compiled program; both are kept alive by the PDO object rooting
    /// the callable, so the bridge stores them without touching the descriptor's
    /// (arena-managed) refcount.
    pub unsafe fn create_function(
        &self,
        name: &str,
        num_args: i64,
        flags: i64,
        descriptor: *mut c_void,
        adapter: *const c_void,
    ) -> i64 {
        let Ok(c_name) = CString::new(name) else {
            return 0;
        };
        let reg = Box::into_raw(Box::new(UdfReg { descriptor, adapter })) as *mut c_void;
        // `_v2` invokes `x_destroy` (freeing the box) even on failure, so only the
        // success path must not free here. `flags` carries SQLITE_DETERMINISTIC etc.,
        // OR-ed into the UTF-8 text encoding as SQLite's C API expects.
        let rc = ffi::sqlite3_create_function_v2(
            self.db,
            c_name.as_ptr(),
            num_args as c_int,
            ffi::SQLITE_UTF8 | (flags as c_int),
            reg,
            Some(x_scalar),
            None,
            None,
            Some(x_destroy),
        );
        if rc == ffi::SQLITE_OK {
            1
        } else {
            0
        }
    }

    /// Registers an aggregate SQL function `name` backed by a compiled-PHP step +
    /// finalize pair (`Pdo\Sqlite::createAggregate`). `num_args` is the declared
    /// arity (-1 = variadic); each callable is decomposed into a descriptor pointer
    /// and the address of its codegen adapter (`__rt_pdo_call_agg_step` /
    /// `__rt_pdo_call_agg_final`). All four pointers are boxed in an `AggReg`
    /// threaded through SQLite's per-registration `pApp`; the per-group accumulator
    /// lives in the aggregate context (`AggCtx`), not here. Returns `1` on success,
    /// `0` on error.
    ///
    /// # Safety
    /// `descriptor`/`adapter` pointers must be the live callable descriptors and
    /// adapter entries of the calling compiled program; both callables are kept alive
    /// by the PDO object rooting them, so the bridge stores them as bare pointers.
    pub unsafe fn create_aggregate(
        &self,
        name: &str,
        num_args: i64,
        step_descriptor: *mut c_void,
        step_adapter: *const c_void,
        final_descriptor: *mut c_void,
        final_adapter: *const c_void,
    ) -> i64 {
        let Ok(c_name) = CString::new(name) else {
            return 0;
        };
        let reg = Box::into_raw(Box::new(AggReg {
            step_descriptor,
            step_adapter,
            final_descriptor,
            final_adapter,
        })) as *mut c_void;
        // An aggregate supplies xStep + xFinal and NULL for xFunc. `_v2` invokes
        // x_destroy_agg (freeing the box) even on failure, so only the success path
        // must not free here. PDO's createAggregate has no DETERMINISTIC/flags arg,
        // so the text encoding is a bare SQLITE_UTF8.
        let rc = ffi::sqlite3_create_function_v2(
            self.db,
            c_name.as_ptr(),
            num_args as c_int,
            ffi::SQLITE_UTF8,
            reg,
            None,
            Some(x_agg_step),
            Some(x_agg_final),
            Some(x_destroy_agg),
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

/// One argument value crossing from the bridge's `x_scalar` shim into the codegen
/// scalar adapter (`__rt_pdo_call_scalar`). A fixed `#[repr(C)]` POD so the adapter
/// can read fields by offset; `tag` selects which payload field is live. `ptr`/`len`
/// alias SQLite's `sqlite3_value` buffers, which stay valid for the whole callback,
/// and the adapter deep-copies them (via `__rt_str_persist`) while boxing, so they
/// need not outlive the call. Offsets (asserted on the codegen side): tag@0, i@8,
/// f@16, ptr@24, len@32.
#[repr(C)]
struct ElephcVal {
    /// 0 = NULL, 1 = INT, 2 = FLOAT, 3 = TEXT, 4 = BLOB.
    tag: i64,
    /// Integer payload (tag 1).
    i: i64,
    /// Float payload (tag 2).
    f: f64,
    /// TEXT/BLOB byte pointer (tags 3/4), aliasing the `sqlite3_value` buffer.
    ptr: *const u8,
    /// TEXT/BLOB byte length (tags 3/4).
    len: i64,
}

/// The scalar user function's return value crossing back from the codegen adapter
/// into `x_scalar`. `#[repr(C)]` POD; offsets tag@0, i@8, f@16. String/blob results
/// do NOT cross as raw pointers: the adapter copies the bytes into the bridge's
/// result stash (`elephc_pdo_udf_stash_bytes`) before releasing its Mixed and sets
/// `tag` to TEXT/BLOB, and `x_scalar` reads the stash. `tag = -1` signals that the
/// callback threw (the adapter's firewall caught it) so `x_scalar` raises a SQL error.
#[repr(C)]
struct ElephcResult {
    /// -1 = ERROR (callback threw), 0 = NULL, 1 = INT, 2 = FLOAT, 3 = TEXT,
    /// 4 = BLOB, 5 = BOOL (0/1 in `i`).
    tag: i64,
    /// Integer / bool payload (tags 1/5).
    i: i64,
    /// Float payload (tag 2).
    f: f64,
}

/// The C-ABI adapter that re-enters a compiled-PHP scalar user function. Emitted by
/// codegen as `__rt_pdo_call_scalar`; the bridge only stores and calls its address.
/// It boxes each `ElephcVal` into a Mixed argument, invokes the callable descriptor's
/// uniform invoker, and writes the return into `*out` (stashing string/blob bytes in
/// the bridge first). A thrown callback is caught by its firewall and reported as
/// `out.tag = -1`.
type ScalarAdapter = unsafe extern "C" fn(
    descriptor: *mut c_void,
    argv: *const ElephcVal,
    argc: i64,
    out: *mut ElephcResult,
);

thread_local! {
    /// Per-thread staging buffer for a scalar/aggregate UDF's string or blob return.
    /// The codegen adapter copies the compiled-PHP string bytes here (they live in the
    /// program's arena and vanish when the adapter returns) via `elephc_pdo_udf_stash_bytes`;
    /// `x_scalar` then hands them to SQLite with `SQLITE_TRANSIENT`. Thread-local because
    /// the adapter runs synchronously on the query's own thread inside the shim.
    static UDF_RESULT_STASH: std::cell::RefCell<(Vec<u8>, bool)> =
        const { std::cell::RefCell::new((Vec::new(), false)) };
}

/// Stages a compiled-PHP UDF string/blob return into the per-thread result stash so
/// `x_scalar` can copy it into SQLite after the adapter releases its Mixed. `is_blob`
/// selects `sqlite3_result_blob` over `_text`. A null pointer or non-positive length
/// stages an empty value.
///
/// # Safety
/// `ptr` must reference `len` readable bytes for the duration of the call.
#[no_mangle]
pub unsafe extern "C" fn elephc_pdo_udf_stash_bytes(ptr: *const u8, len: i64, is_blob: i64) {
    let bytes = if ptr.is_null() || len <= 0 {
        Vec::new()
    } else {
        std::slice::from_raw_parts(ptr, len as usize).to_vec()
    };
    UDF_RESULT_STASH.with(|stash| *stash.borrow_mut() = (bytes, is_blob != 0));
}

/// Takes and clears the staged UDF string/blob return `(bytes, is_blob)`.
fn udf_result_stash_take() -> (Vec<u8>, bool) {
    UDF_RESULT_STASH.with(|stash| std::mem::take(&mut *stash.borrow_mut()))
}

/// Clears any stale staged UDF result before invoking a callback.
fn udf_result_stash_clear() {
    UDF_RESULT_STASH.with(|stash| {
        let mut stash = stash.borrow_mut();
        stash.0.clear();
        stash.1 = false;
    });
}

/// Decodes one `sqlite3_value` into an `ElephcVal`, mirroring the statement fetch
/// path's byte-counted read (`sqlite3_value_blob` + `_bytes`) so TEXT/BLOB arguments
/// with embedded NUL bytes round-trip exactly.
///
/// # Safety
/// `v` must be a live `sqlite3_value` valid for the current callback.
unsafe fn decode_value(v: *mut ffi::sqlite3_value) -> ElephcVal {
    match ffi::sqlite3_value_type(v) {
        1 => ElephcVal {
            tag: 1,
            i: ffi::sqlite3_value_int64(v),
            f: 0.0,
            ptr: std::ptr::null(),
            len: 0,
        },
        2 => ElephcVal {
            tag: 2,
            i: 0,
            f: ffi::sqlite3_value_double(v),
            ptr: std::ptr::null(),
            len: 0,
        },
        code @ (3 | 4) => {
            let ptr = ffi::sqlite3_value_blob(v) as *const u8;
            let len = ffi::sqlite3_value_bytes(v).max(0) as i64;
            ElephcVal {
                tag: code as i64,
                i: 0,
                f: 0.0,
                ptr,
                len,
            }
        }
        _ => ElephcVal {
            tag: 0,
            i: 0,
            f: 0.0,
            ptr: std::ptr::null(),
            len: 0,
        },
    }
}

/// Writes an `ElephcResult` into the SQLite call context via the `sqlite3_result_*`
/// family. String/blob results are copied out of the per-thread stash with
/// `SQLITE_TRANSIENT` (SQLite owns its own copy); a `-1` tag raises a SQL error.
///
/// # Safety
/// `ctx` must be the live `sqlite3_context` for the current callback.
unsafe fn dispatch_scalar_result(ctx: *mut ffi::sqlite3_context, out: &ElephcResult) {
    match out.tag {
        -1 => {
            let msg = c"PDO user function callback raised an exception";
            ffi::sqlite3_result_error(ctx, msg.as_ptr(), -1);
        }
        1 | 5 => ffi::sqlite3_result_int64(ctx, out.i),
        2 => ffi::sqlite3_result_double(ctx, out.f),
        3 | 4 => {
            let (bytes, is_blob) = udf_result_stash_take();
            if is_blob || out.tag == 4 {
                ffi::sqlite3_result_blob(
                    ctx,
                    bytes.as_ptr() as *const c_void,
                    bytes.len() as c_int,
                    ffi::SQLITE_TRANSIENT(),
                );
            } else {
                ffi::sqlite3_result_text(
                    ctx,
                    bytes.as_ptr() as *const c_char,
                    bytes.len() as c_int,
                    ffi::SQLITE_TRANSIENT(),
                );
            }
        }
        _ => ffi::sqlite3_result_null(ctx),
    }
}

/// SQLite scalar user-function dispatcher (`xFunc`). Unlike `x_compare`, a scalar
/// callback receives no `pApp` argument, so the `UdfReg` is recovered through
/// `sqlite3_user_data`. Each argument is decoded into an `ElephcVal`, the codegen
/// adapter re-enters the compiled-PHP callable, and its `ElephcResult` is written
/// back through the `sqlite3_result_*` family.
///
/// # Safety
/// `ctx`/`argv` are the live SQLite call context and argument vector; the registered
/// `pApp` is a live `Box<UdfReg>` pointer.
unsafe extern "C" fn x_scalar(
    ctx: *mut ffi::sqlite3_context,
    argc: c_int,
    argv: *mut *mut ffi::sqlite3_value,
) {
    let p_arg = ffi::sqlite3_user_data(ctx);
    if p_arg.is_null() {
        ffi::sqlite3_result_null(ctx);
        return;
    }
    let reg = &*(p_arg as *const UdfReg);
    let mut vals: Vec<ElephcVal> = Vec::with_capacity(argc.max(0) as usize);
    for idx in 0..argc {
        vals.push(decode_value(*argv.offset(idx as isize)));
    }
    let adapter: ScalarAdapter = std::mem::transmute(reg.adapter);
    let mut out = ElephcResult {
        tag: 0,
        i: 0,
        f: 0.0,
    };
    udf_result_stash_clear();
    adapter(reg.descriptor, vals.as_ptr(), vals.len() as i64, &mut out);
    dispatch_scalar_result(ctx, &out);
}

/// A registered SQLite aggregate: the step and finalize callables each as a
/// (descriptor, adapter) pair. Boxed and handed to SQLite as the registration's
/// `pApp`, recovered by both `x_agg_step` and `x_agg_final` via
/// `sqlite3_user_data`, and freed by `x_destroy_agg`. Distinct from `UdfReg`
/// (which holds a single pair): an aggregate needs both callables, so a
/// same-`pApp` widening would mis-size the `Box` free — hence a separate struct
/// and a separate destroy.
#[repr(C)]
struct AggReg {
    /// The step callable's compiled-PHP descriptor pointer.
    step_descriptor: *mut c_void,
    /// The codegen step adapter entry (`__rt_pdo_call_agg_step`).
    step_adapter: *const c_void,
    /// The finalize callable's compiled-PHP descriptor pointer.
    final_descriptor: *mut c_void,
    /// The codegen finalize adapter entry (`__rt_pdo_call_agg_final`).
    final_adapter: *const c_void,
}

/// The per-group aggregate state SQLite keeps in `sqlite3_aggregate_context`. A
/// `#[repr(C)]` POD so the fixed 16-byte block is shared unambiguously across step
/// calls and the final call within one aggregation group. `row_count` is the
/// running number of `xStep` invocations so far (the `$rownumber` passed to the
/// callbacks); `accumulator` is the boxed-Mixed PHP value the last step returned
/// (null before the first step). SQLite owns and auto-frees this 16-byte block when
/// the aggregation concludes; the bridge/adapters own the pointed-to accumulator box
/// (which lives in the compiled program's heap) and release it inside `x_agg_final`.
#[repr(C)]
struct AggCtx {
    /// Running `xStep` count within the group (0 before the first step).
    row_count: i64,
    /// The boxed-Mixed accumulator the last step returned (null = none yet).
    accumulator: *mut c_void,
}

/// The C-ABI adapter that re-enters a compiled-PHP aggregate step callback. Emitted
/// by codegen as `__rt_pdo_call_agg_step`; the bridge only stores and calls its
/// address. It boxes `[accumulator, rownumber, ...rowValues]` as the invoker's
/// arguments, invokes the step callable, and returns the OWNED boxed-Mixed new
/// accumulator (the bridge stores it back into `AggCtx.accumulator`). On a thrown
/// callback the adapter's firewall catches the longjmp, preserves the accumulator
/// (so `x_agg_final` still frees it), sets `*threw = 1`, and returns null.
type StepAdapter = unsafe extern "C" fn(
    descriptor: *mut c_void,
    accumulator: *mut c_void,
    rownumber: i64,
    argv: *const ElephcVal,
    argc: i64,
    threw: *mut i64,
) -> *mut c_void;

/// The C-ABI adapter that re-enters a compiled-PHP aggregate finalize callback.
/// Emitted by codegen as `__rt_pdo_call_agg_final`; the bridge only stores and calls
/// its address. It boxes `[accumulator, rownumber]`, invokes the finalize callable,
/// writes the aggregate result into `*out` (an `ElephcResult`, decoded exactly like
/// the scalar path), and — since finalize is terminal for the group — releases the
/// accumulator box. A thrown callback is reported as `out.tag = -1`.
type FinalAdapter = unsafe extern "C" fn(
    descriptor: *mut c_void,
    accumulator: *mut c_void,
    rownumber: i64,
    out: *mut ElephcResult,
);

/// SQLite aggregate step dispatcher (`xStep`). Recovers the `AggReg` via
/// `sqlite3_user_data` and the per-group `AggCtx` via `sqlite3_aggregate_context`
/// (16 bytes, zeroed on the first step of a group). Decodes the row arguments, calls
/// the codegen step adapter with the current accumulator + row number, and stores the
/// new accumulator back. A thrown step callback (`threw != 0`) surfaces a SQL error;
/// SQLite then aborts the aggregation but still runs `xFinal`, which frees the
/// accumulator the adapter preserved.
///
/// # Safety
/// `ctx`/`argv` are the live SQLite call context and argument vector; the registered
/// `pApp` is a live `Box<AggReg>` pointer.
unsafe extern "C" fn x_agg_step(
    ctx: *mut ffi::sqlite3_context,
    argc: c_int,
    argv: *mut *mut ffi::sqlite3_value,
) {
    let p_arg = ffi::sqlite3_user_data(ctx);
    if p_arg.is_null() {
        return;
    }
    let reg = &*(p_arg as *const AggReg);
    let slot =
        ffi::sqlite3_aggregate_context(ctx, std::mem::size_of::<AggCtx>() as c_int) as *mut AggCtx;
    if slot.is_null() {
        // Out of memory: the group cannot be aggregated. SQLite reports the OOM.
        return;
    }
    let mut vals: Vec<ElephcVal> = Vec::with_capacity(argc.max(0) as usize);
    for idx in 0..argc {
        vals.push(decode_value(*argv.offset(idx as isize)));
    }
    // PHP (`sqlite_driver.c`: `ZVAL_LONG(&zargs[1], ++agg_context->row)`) pre-increments
    // the shared row counter before passing it to the callback, so `$rownumber` runs
    // 1..N across the group's steps (never 0). Increment first, then pass.
    (*slot).row_count += 1;
    let adapter: StepAdapter = std::mem::transmute(reg.step_adapter);
    let mut threw: i64 = 0;
    let new_acc = adapter(
        reg.step_descriptor,
        (*slot).accumulator,
        (*slot).row_count,
        vals.as_ptr(),
        vals.len() as i64,
        &mut threw,
    );
    if threw != 0 {
        // The adapter preserved the accumulator (did not release the slot's ref) and
        // returned null. Surface a SQL error; xFinal still runs and frees it.
        let msg = c"PDO aggregate step callback raised an exception";
        ffi::sqlite3_result_error(ctx, msg.as_ptr(), -1);
    } else {
        (*slot).accumulator = new_acc;
    }
}

/// SQLite aggregate finalize dispatcher (`xFinal`). Recovers the `AggReg` and reads
/// the per-group `AggCtx` with `sqlite3_aggregate_context(ctx, 0)` — passing 0 so an
/// empty group (no `xStep` ever ran) returns a NULL slot rather than allocating,
/// which the dispatcher treats as `{row_count: 0, accumulator: null}` (PHP null
/// context) before pre-incrementing the row number it hands to the adapter (see
/// below). Calls the codegen finalize adapter to produce the result and release the
/// accumulator, writes it through `dispatch_scalar_result`, then nulls the slot so no
/// freed pointer dangles before SQLite frees the block.
///
/// # Safety
/// `ctx` is the live SQLite call context; the registered `pApp` is a live
/// `Box<AggReg>` pointer.
unsafe extern "C" fn x_agg_final(ctx: *mut ffi::sqlite3_context) {
    let p_arg = ffi::sqlite3_user_data(ctx);
    if p_arg.is_null() {
        ffi::sqlite3_result_null(ctx);
        return;
    }
    let reg = &*(p_arg as *const AggReg);
    // nBytes 0: do NOT allocate for an empty group; a NULL slot means "never stepped".
    let slot = ffi::sqlite3_aggregate_context(ctx, 0) as *mut AggCtx;
    // PHP pre-increments the SAME shared row counter for the finalize call too
    // (`++agg_context->row`), so finalize sees one past the last step's rownumber
    // (N+1 for an N-step group), or 1 for an empty group (the counter starts at 0
    // and is pre-incremented even though xStep never ran).
    let (accumulator, row_count) = if slot.is_null() {
        (ptr::null_mut(), 1i64)
    } else {
        ((*slot).accumulator, (*slot).row_count + 1)
    };
    let adapter: FinalAdapter = std::mem::transmute(reg.final_adapter);
    let mut out = ElephcResult {
        tag: 0,
        i: 0,
        f: 0.0,
    };
    udf_result_stash_clear();
    adapter(reg.final_descriptor, accumulator, row_count, &mut out);
    dispatch_scalar_result(ctx, &out);
    // The adapter released the accumulator box; null the slot so the now-freed
    // pointer never dangles (SQLite frees the 16-byte block right after this returns).
    if !slot.is_null() {
        (*slot).accumulator = ptr::null_mut();
    }
}

/// Frees a `Box<AggReg>` when SQLite deletes an aggregate registration. Registered as
/// every aggregate's `xDestroy`. Distinct from `x_destroy` (which frees a `UdfReg`):
/// `AggReg` is a different, larger type, so freeing it through the wrong `Box` type
/// would pass a mismatched `Layout` to the allocator.
///
/// # Safety
/// `p_arg` must be a pointer produced by `Box::into_raw` for an `AggReg`.
unsafe extern "C" fn x_destroy_agg(p_arg: *mut c_void) {
    if !p_arg.is_null() {
        drop(Box::from_raw(p_arg as *mut AggReg));
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

    /// Returns `1` if the statement makes no direct changes to the content of
    /// the database file (`sqlite3_stmt_readonly`), else `0`. Backs
    /// `PDOStatement::getAttribute(Pdo\Sqlite::ATTR_READONLY_STATEMENT)` (P2-16)
    /// as a live read rather than a stored value.
    pub fn readonly(&self) -> i64 {
        (unsafe { ffi::sqlite3_stmt_readonly(self.ptr) } != 0) as i64
    }
}
