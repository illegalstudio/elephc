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
//! - Handle ownership: `SqliteConn` owns its `sqlite3*` and `SqliteStmt` owns its
//!   `sqlite3_stmt*` (it only *borrows* the connection's `sqlite3*`). Each native
//!   handle is released exactly once — by the explicit `close()` / `finalize()`
//!   that `lib.rs` calls, with an `impl Drop` as the structural safety net for any
//!   path that drops one of these values without calling them.
//! - Thread safety: the bridge locks its connection and statement tables under two
//!   SEPARATE mutexes, so overlapping calls on one `sqlite3*` are only defined
//!   under a mutexed SQLite build; `assert_sqlite_threadsafe` pins that invariant
//!   at the first open.

use std::cell::{Cell, RefCell};
use std::ffi::{CStr, CString};
use std::os::raw::{c_char, c_int, c_void};
use std::ptr;
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::{Arc, Once};

use libsqlite3_sys as ffi;

use crate::ffi_guard;

/// A live SQLite connection. The raw pointer is `Send` in practice because
/// elephc programs drive one connection from one thread at a time.
///
/// The struct OWNS its `sqlite3*`: the handle is released exactly once, either by
/// the explicit `close()` (`elephc_pdo_close`'s only call site, which has to run
/// first because it finalizes the connection's statements) or, failing that, by
/// the `Drop` net below. `released` is what makes those two idempotent with
/// respect to each other. It is a separate flag rather than a null-out of `db`
/// because `close()` only ever holds a `&self` — `lib.rs` reaches it through
/// `HashMap::get` on the connection table — and because `db` has to stay a plain
/// `*mut` field that `lib.rs` can read out by `Copy` to decide which registered
/// statements belong to this connection.
pub struct SqliteConn {
    /// The owned native connection handle.
    pub db: *mut ffi::sqlite3,
    /// Whether `db` has already been handed back to SQLite (see the type docs).
    released: Cell<bool>,
    /// Transaction opening mode used by the next `PDO::beginTransaction()` call.
    transaction_mode: Cell<i64>,
    /// Authorizer callback registration owned by this connection. SQLite's
    /// authorizer API has no destructor hook, so replacement/reset/close free it
    /// explicitly rather than using the UDF registration path's `x_destroy`.
    authorizer: Cell<*mut AuthorizerReg>,
    /// Deferred PHP error classification produced inside SQLite's C callback.
    authorizer_error: Arc<AtomicI64>,
    /// Successfully registered collation names whose native callbacks must be
    /// removed before PHP releases their callable descriptor roots.
    collations: RefCell<Vec<String>>,
    /// Successfully registered scalar/aggregate `(name, arity)` pairs. SQLite
    /// shares one namespace for both forms, so one key tracks either registration.
    functions: RefCell<Vec<(String, i64)>>,
}
unsafe impl Send for SqliteConn {}

impl Drop for SqliteConn {
    /// Defense-in-depth release of the native handle, alongside (not instead of)
    /// the explicit `close()`. Rust's default drop of a bare raw pointer is a
    /// no-op, so without this a future path that merely drops a `SqliteConn` — one
    /// built but never registered, or removed from the connection table some other
    /// way — would leak the handle with no crash and no warning.
    ///
    /// Sound because a `SqliteConn` is never cloned or copied (it has no such impl,
    /// and a `Drop` type cannot be `Copy`) and is only ever *moved* — into
    /// `Conn::Sqlite`, then into the connection table — and a move never drops its
    /// source. Both release paths go through `released`, so after a successful
    /// `close()` this is a no-op and `elephc_pdo_close`'s close-then-remove sequence
    /// still frees the handle exactly once.
    fn drop(&mut self) {
        if self.released.replace(true) || self.db.is_null() {
            return;
        }
        self.clear_callbacks();
        // `sqlite3_close` (not `_v2`) declines with SQLITE_BUSY when statements are
        // still live rather than freeing the handle underneath them, so this net can
        // never yank a db out from under a statement that is still registered: at
        // worst it degrades to the very leak it exists to prevent, never to a
        // use-after-free.
        unsafe { ffi::sqlite3_close(self.db) };
        self.db = ptr::null_mut();
    }
}

/// A live SQLite prepared statement plus the connection pointer it belongs to.
///
/// The struct OWNS `ptr` but only BORROWS `db`, which stays owned by the
/// `SqliteConn` — hence the asymmetry in `Drop`, which finalizes the statement and
/// never touches the connection. `released` guards `ptr` exactly as `SqliteConn`'s
/// flag guards `db`.
pub struct SqliteStmt {
    /// The owned native statement handle.
    pub ptr: *mut ffi::sqlite3_stmt,
    /// The connection the statement was prepared on. Borrowed, never released here:
    /// the error accessors need it because SQLite tracks error state per-connection.
    pub db: *mut ffi::sqlite3,
    /// Whether `ptr` has already been handed back to SQLite (see the type docs).
    released: Cell<bool>,
}
unsafe impl Send for SqliteStmt {}

impl Drop for SqliteStmt {
    /// Defense-in-depth finalize of the native statement, alongside (not instead of)
    /// the explicit `finalize()`, for the same reason `SqliteConn`'s `Drop` exists:
    /// a dropped raw pointer releases nothing.
    ///
    /// Sound because a `SqliteStmt` is never cloned or copied and is only ever moved
    /// (out of `prepare`, into `Stmt::Sqlite`, into the statement table), because
    /// `released` makes it a no-op after `elephc_pdo_finalize`'s explicit
    /// `finalize()`, and because it releases only `ptr` — `db` is the connection's
    /// handle, owned by `SqliteConn`. `sqlite3_finalize` also needs its connection
    /// still open, which holds on every path: `elephc_pdo_finalize` drops one
    /// statement while its connection stays registered, `elephc_pdo_close` finalizes
    /// and drops *every* statement of a connection before closing it, and the global
    /// tables are `OnceLock` statics that are never dropped at process exit, so no
    /// teardown order can invert that.
    fn drop(&mut self) {
        if self.released.replace(true) || self.ptr.is_null() {
            return;
        }
        unsafe { ffi::sqlite3_finalize(self.ptr) };
        self.ptr = ptr::null_mut();
    }
}

/// Checks once, at the first connection open, that the linked SQLite is not the
/// mutex-free build. The bridge locks its connection table and its statement table
/// under two SEPARATE mutexes (`lib.rs`'s `conns()` / `stmts()`), so nothing stops
/// an `sqlite3_step()` driven from one thread from overlapping an `sqlite3_exec()`
/// on the same `sqlite3*` from another; that overlap is only defined when SQLite
/// serializes API entry on the connection's own mutex (`SQLITE_THREADSAFE=1`).
/// `libsqlite3-sys`'s `bundled` feature — which this crate pins — compiles the
/// amalgamation with `-DSQLITE_THREADSAFE=1`, so the invariant holds by
/// construction today; the assertion pins it against a later switch to a system
/// SQLite or a hand-set compile flag, which would otherwise corrupt data silently
/// instead of failing. `sqlite3_threadsafe()` only reports whether the mutex code
/// was compiled in, so it rules out the single-threaded build (the one that is
/// actually unsound here) rather than proving the serialized mode specifically —
/// the mutex-free build is the only variant `bundled` could realistically produce.
///
/// The check is skipped on wasm, where `libsqlite3-sys` deliberately builds the
/// amalgamation with `SQLITE_THREADSAFE=0`: that target has no threads, so the
/// overlap this invariant guards against cannot arise there.
fn assert_sqlite_threadsafe() {
    static CHECKED: Once = Once::new();
    CHECKED.call_once(|| {
        #[cfg(not(target_family = "wasm"))]
        assert!(
            unsafe { ffi::sqlite3_threadsafe() } != 0,
            "elephc-pdo requires a thread-safe SQLite build: the bridge locks its connection \
             and statement tables separately, so calls on one sqlite3* can overlap across \
             threads, which is only safe under SQLITE_THREADSAFE=1 (serialized)",
        );
    });
}

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
        ffi::SQLITE_NOLFS => "HYC00",
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
        assert_sqlite_threadsafe();
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
        Ok(SqliteConn {
            db,
            released: Cell::new(false),
            transaction_mode: Cell::new(0),
            authorizer: Cell::new(ptr::null_mut()),
            authorizer_error: Arc::new(AtomicI64::new(0)),
            collations: RefCell::new(Vec::new()),
            functions: RefCell::new(Vec::new()),
        })
    }

    /// Runs SQL on a borrowed native connection pointer without requiring the
    /// bridge's connection-table lock to remain held across SQLite callbacks.
    ///
    /// # Safety
    /// `db` must be a live SQLite connection and `sql` a valid NUL-terminated string.
    pub unsafe fn exec_on(db: *mut ffi::sqlite3, sql: *const c_char) -> i64 {
        if sql.is_null() {
            return -1;
        }
        let rc = ffi::sqlite3_exec(db, sql, None, ptr::null_mut(), ptr::null_mut());
        if rc != ffi::SQLITE_OK {
            return -1;
        }
        ffi::sqlite3_changes(db) as i64
    }

    /// Returns the rowid of the most recent successful INSERT.
    pub fn last_insert_id(&self) -> i64 {
        unsafe { ffi::sqlite3_last_insert_rowid(self.db) }
    }

    /// Returns the number of rows changed by the most recent statement.
    pub fn changes(&self) -> i64 {
        unsafe { ffi::sqlite3_changes(self.db) as i64 }
    }

    /// Returns whether the connection is currently inside a transaction, read
    /// live from SQLite's own autocommit flag (`sqlite3_get_autocommit`) rather
    /// than any PHP-side bookkeeping (P1-g). SQLite reports non-autocommit (`0`)
    /// from the moment a `BEGIN` — issued via `PDO::beginTransaction()` OR a raw
    /// `PDO::exec("BEGIN")` — takes effect until the matching `COMMIT`/`ROLLBACK`
    /// (or an auto-rollback on error), so this mirrors php-src's own
    /// `pdo_sqlite3_in_transaction` handler exactly. Returns `1` when a
    /// transaction is active, `0` otherwise.
    pub fn in_transaction(&self) -> i64 {
        (unsafe { ffi::sqlite3_get_autocommit(self.db) } == 0) as i64
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

    /// Begins a transaction with the configured PHP 8.5 SQLite transaction mode.
    pub fn begin_transaction(&self) -> i64 {
        let sql = match self.transaction_mode.get() {
            1 => b"BEGIN IMMEDIATE".as_slice(),
            2 => b"BEGIN EXCLUSIVE".as_slice(),
            _ => b"BEGIN DEFERRED".as_slice(),
        };
        self.exec_simple(sql)
    }

    /// Stores a validated PHP 8.5 SQLite transaction mode, returning `1` on success.
    pub fn set_transaction_mode(&self, mode: i64) -> i64 {
        if !(0..=2).contains(&mode) {
            return 0;
        }
        self.transaction_mode.set(mode);
        1
    }

    /// Returns the configured PHP 8.5 SQLite transaction mode.
    pub fn transaction_mode(&self) -> i64 {
        self.transaction_mode.get()
    }

    /// Returns SQLite's primary result code for the connection's last operation.
    pub fn errcode(&self) -> i64 {
        unsafe { ffi::sqlite3_errcode(self.db) as i64 }
    }

    /// Returns the connection's current error message.
    pub fn errmsg(&self) -> String {
        unsafe { read_errmsg(self.db) }
    }

    /// Prepares SQL on a borrowed native connection pointer without retaining the
    /// bridge's connection-table lock while an authorizer callback runs.
    ///
    /// # Safety
    /// `db` must be a live SQLite connection and `sql` a valid NUL-terminated string.
    pub unsafe fn prepare_on(
        db: *mut ffi::sqlite3,
        sql: *const c_char,
    ) -> Result<SqliteStmt, ()> {
        if sql.is_null() {
            return Err(());
        }
        let mut stmt: *mut ffi::sqlite3_stmt = ptr::null_mut();
        // -1 length lets SQLite read up to the NUL terminator.
        let rc = ffi::sqlite3_prepare_v2(db, sql, -1, &mut stmt, ptr::null_mut());
        if rc != ffi::SQLITE_OK || stmt.is_null() {
            return Err(());
        }
        Ok(SqliteStmt {
            ptr: stmt,
            db,
            released: Cell::new(false),
        })
    }

    /// Closes the connection (the caller finalizes its statements first), releasing
    /// the native handle. Idempotent: a second call — or the `Drop` net — is a no-op
    /// once SQLite has taken the handle back, so it is freed exactly once.
    pub fn close(&self) {
        if self.released.get() || self.db.is_null() {
            return;
        }
        self.clear_callbacks();
        // Only SQLITE_OK means SQLite actually freed the handle. Anything else
        // (SQLITE_BUSY: a statement of this connection outlived the caller's
        // finalize loop) leaves it live and un-released, so `Drop` still gets a shot
        // at it — re-closing a live handle is safe, re-closing a freed one would be
        // a use-after-free.
        if unsafe { ffi::sqlite3_close(self.db) } == ffi::SQLITE_OK {
            self.released.set(true);
        }
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

    /// Turns SQLite's extended result codes on (`on != 0`) or off, backing
    /// `PDO::setAttribute(Pdo\Sqlite::ATTR_EXTENDED_RESULT_CODES, …)`. php-src's
    /// `pdo_sqlite_set_attr` (`ext/pdo_sqlite/sqlite_driver.c`) makes exactly this
    /// `sqlite3_extended_result_codes(H->db, lval)` call: with extended codes on,
    /// `sqlite3_errcode()` — the value PDO reports as `errorInfo[1]` — returns the
    /// refined code (2067 `SQLITE_CONSTRAINT_UNIQUE`) where it would otherwise
    /// return the primary one (19 `SQLITE_CONSTRAINT`). Returns `1` on `SQLITE_OK`,
    /// `0` otherwise.
    ///
    /// `sqlite_sqlstate` is deliberately left keying off the unmasked code, so an
    /// extended code falls through its match to the generic `HY000`. That is not an
    /// oversight: php-src's `pdo_sqlite_error` switches on the same unmasked
    /// `sqlite3_errcode()` value, so its SQLSTATE degrades identically once the
    /// attribute is on.
    pub fn set_extended_result_codes(&self, on: i64) -> i64 {
        let rc = unsafe { ffi::sqlite3_extended_result_codes(self.db, (on != 0) as c_int) };
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

    /// Returns SQLite's linked library version, which php-src exposes identically
    /// for both its client- and server-version attributes.
    pub fn client_version(&self) -> String {
        self.server_version()
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
    /// surfaces as `Err(message)`. Backs the initial snapshot used by
    /// `Pdo\Sqlite::openBlob()`'s seekable stream wrapper.
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
            // flags = 0 is sufficient for the wrapper's initial snapshot; writable
            // range updates reopen the same cell through `blob_write`.
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

    /// Replaces the bytes of an existing SQLite BLOB through the incremental-blob
    /// API. SQLite cannot resize an incremental BLOB, so `data` must have exactly
    /// the cell's existing byte length; callers implement partial writes by first
    /// reading the cell, patching that snapshot, and sending the full fixed-size
    /// value back. Returns `Ok(())` on success and the live SQLite error otherwise.
    pub fn blob_write(
        &self,
        dbname: &str,
        table: &str,
        column: &str,
        rowid: i64,
        data: &[u8],
    ) -> Result<(), String> {
        let c_db = CString::new(dbname).map_err(|_| "invalid database name".to_string())?;
        let c_table = CString::new(table).map_err(|_| "invalid table name".to_string())?;
        let c_col = CString::new(column).map_err(|_| "invalid column name".to_string())?;
        unsafe {
            let mut blob: *mut ffi::sqlite3_blob = ptr::null_mut();
            let rc = ffi::sqlite3_blob_open(
                self.db,
                c_db.as_ptr(),
                c_table.as_ptr(),
                c_col.as_ptr(),
                rowid,
                1,
                &mut blob,
            );
            if rc != ffi::SQLITE_OK || blob.is_null() {
                return Err(read_errmsg(self.db));
            }
            let size = ffi::sqlite3_blob_bytes(blob).max(0) as usize;
            if size != data.len() {
                ffi::sqlite3_blob_close(blob);
                return Err("It is not possible to increase the size of a BLOB".to_string());
            }
            let write_rc = if data.is_empty() {
                ffi::SQLITE_OK
            } else {
                ffi::sqlite3_blob_write(
                    blob,
                    data.as_ptr() as *const c_void,
                    data.len() as c_int,
                    0,
                )
            };
            let err = (write_rc != ffi::SQLITE_OK).then(|| read_errmsg(self.db));
            ffi::sqlite3_blob_close(blob);
            match err {
                Some(msg) => Err(msg),
                None => Ok(()),
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
            let mut collations = self.collations.borrow_mut();
            collations.retain(|registered| !registered.eq_ignore_ascii_case(name));
            collations.push(name.to_string());
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
            self.remember_function(name, num_args);
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
            self.remember_function(name, num_args);
            1
        } else {
            0
        }
    }

    /// Installs a PHP 8.5 SQLite authorizer backed by a compiled-PHP callable.
    /// The scalar callback adapter is reused because the authorizer's five values
    /// use the same int/string/null argument shape and boxed scalar return ABI.
    /// Replacing a callback releases the previous registration. Returns `1` on
    /// success and `0` when SQLite rejects the registration.
    ///
    /// # Safety
    /// `descriptor` and `adapter` must remain valid while the authorizer is
    /// installed. The PDO prelude roots the descriptor for that lifetime.
    pub unsafe fn set_authorizer(
        &self,
        descriptor: *mut c_void,
        adapter: *const c_void,
    ) -> i64 {
        self.clear_authorizer();
        let reg = Box::into_raw(Box::new(AuthorizerReg {
            descriptor,
            adapter,
            error: Arc::clone(&self.authorizer_error),
        }));
        let rc = ffi::sqlite3_set_authorizer(self.db, Some(x_authorizer), reg as *mut c_void);
        if rc == ffi::SQLITE_OK {
            self.authorizer.set(reg);
            1
        } else {
            drop(Box::from_raw(reg));
            0
        }
    }

    /// Removes and frees the installed SQLite authorizer, if any. This is
    /// idempotent and is used by nullable reset, replacement, close, and `Drop`.
    pub fn clear_authorizer(&self) {
        let reg = self.authorizer.replace(ptr::null_mut());
        unsafe {
            ffi::sqlite3_set_authorizer(self.db, None, ptr::null_mut());
            if !reg.is_null() {
                drop(Box::from_raw(reg));
            }
        }
        self.authorizer_error.store(0, Ordering::Release);
    }

    /// Removes every callback registration before its compiled-PHP descriptor roots
    /// are released. This is required even for persistent handles, which stay in the
    /// pool after the owning PDO object is destroyed.
    pub fn clear_callbacks(&self) {
        self.clear_authorizer();
        let collations = self.collations.take();
        for name in collations {
            let Ok(c_name) = CString::new(name) else {
                continue;
            };
            unsafe {
                ffi::sqlite3_create_collation_v2(
                    self.db,
                    c_name.as_ptr(),
                    ffi::SQLITE_UTF8,
                    ptr::null_mut(),
                    None,
                    None,
                );
            }
        }
        let functions = self.functions.take();
        for (name, num_args) in functions {
            let Ok(c_name) = CString::new(name) else {
                continue;
            };
            unsafe {
                ffi::sqlite3_create_function_v2(
                    self.db,
                    c_name.as_ptr(),
                    num_args as c_int,
                    ffi::SQLITE_UTF8,
                    ptr::null_mut(),
                    None,
                    None,
                    None,
                    None,
                );
            }
        }
    }

    /// Records one successful scalar or aggregate registration, replacing the
    /// case-insensitive SQLite key already occupying the same name and arity.
    fn remember_function(&self, name: &str, num_args: i64) {
        let mut functions = self.functions.borrow_mut();
        functions.retain(|(registered, arity)| {
            *arity != num_args || !registered.eq_ignore_ascii_case(name)
        });
        functions.push((name.to_string(), num_args));
    }

    /// Takes and clears a deferred authorizer callback error classification.
    /// Zero means the callback returned a valid SQLite decision or did not run.
    pub fn take_authorizer_error(&self) -> i64 {
        self.authorizer_error.swap(0, Ordering::AcqRel)
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

/// SQLite authorizer registration with deferred PHP error state. The authorizer
/// API has no destructor hook, so `SqliteConn` owns and frees this box directly.
struct AuthorizerReg {
    /// The 64-byte compiled-PHP callable descriptor pointer.
    descriptor: *mut c_void,
    /// The shared scalar callback adapter entry.
    adapter: *const c_void,
    /// Error classification consumed by the outer PDO method after SQLite returns.
    error: Arc<AtomicI64>,
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

/// Converts one nullable, NUL-terminated SQLite authorizer argument to the
/// byte-counted value record consumed by the shared scalar callback adapter.
///
/// # Safety
/// A non-null `value` must point to a live NUL-terminated SQLite string for the
/// duration of the current authorizer callback.
unsafe fn decode_nullable_cstr(value: *const c_char) -> ElephcVal {
    if value.is_null() {
        return ElephcVal {
            tag: 0,
            i: 0,
            f: 0.0,
            ptr: ptr::null(),
            len: 0,
        };
    }
    let bytes = CStr::from_ptr(value).to_bytes();
    ElephcVal {
        tag: 3,
        i: 0,
        f: 0.0,
        ptr: bytes.as_ptr(),
        len: bytes.len() as i64,
    }
}

/// SQLite authorizer dispatcher. It forwards the action code and four nullable
/// context strings to the compiled-PHP callable and accepts only the three integer
/// decisions SQLite defines (`OK`, `DENY`, and `IGNORE`). Exceptions and invalid
/// return types/values fail closed with `SQLITE_DENY`.
///
/// # Safety
/// `p_arg` must be a live `Box<AuthorizerReg>` installed by `set_authorizer`; every
/// non-null string pointer is owned by SQLite and valid for this callback.
unsafe extern "C" fn x_authorizer(
    p_arg: *mut c_void,
    action: c_int,
    arg1: *const c_char,
    arg2: *const c_char,
    arg3: *const c_char,
    arg4: *const c_char,
) -> c_int {
    if p_arg.is_null() {
        return ffi::SQLITE_OK;
    }
    let reg = &*(p_arg as *const AuthorizerReg);
    let values = [
        ElephcVal {
            tag: 1,
            i: action as i64,
            f: 0.0,
            ptr: ptr::null(),
            len: 0,
        },
        decode_nullable_cstr(arg1),
        decode_nullable_cstr(arg2),
        decode_nullable_cstr(arg3),
        decode_nullable_cstr(arg4),
    ];
    let adapter: ScalarAdapter = std::mem::transmute(reg.adapter);
    let mut out = ElephcResult {
        tag: 0,
        i: 0,
        f: 0.0,
    };
    udf_result_stash_clear();
    adapter(
        reg.descriptor,
        values.as_ptr(),
        values.len() as i64,
        &mut out,
    );
    let error = match out.tag {
        1 if matches!(out.i, 0..=2) => 0,
        1 => 1,
        -1 => 2,
        0 => 10,
        2 => 11,
        3 | 4 => 12,
        5 => 13,
        6 => 14,
        7 => 15,
        _ => 15,
    };
    reg.error.store(error, Ordering::Release);
    if error == 0 {
        out.i as c_int
    } else {
        ffi::SQLITE_DENY
    }
}

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
/// stages an empty value, and so does a caught panic — `x_scalar` then hands SQLite an
/// empty result rather than aborting the process.
///
/// [`ffi_guard`] wraps this like every other `#[no_mangle]` body (F-QUAL-02): it is the
/// one bridge entry point outside `lib.rs`, and it is reached from a compiled-PHP UDF
/// callback running inside SQLite's own call stack — so an unwind out of it would cross
/// TWO `extern "C"` frames (this one and SQLite's `xFunc`) and abort.
///
/// # Safety
/// `ptr` must reference `len` readable bytes for the duration of the call.
#[no_mangle]
pub unsafe extern "C" fn elephc_pdo_udf_stash_bytes(ptr: *const u8, len: i64, is_blob: i64) {
    ffi_guard((), || {
        let bytes = if ptr.is_null() || len <= 0 {
            Vec::new()
        } else {
            std::slice::from_raw_parts(ptr, len as usize).to_vec()
        };
        UDF_RESULT_STASH.with(|stash| *stash.borrow_mut() = (bytes, is_blob != 0));
    })
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
        6 | 7 => {
            let msg = c"PDO user function callback returned an unsupported type";
            ffi::sqlite3_result_error(ctx, msg.as_ptr(), -1);
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
    /// Returns SQLite's source table name for result column `i`, or an empty
    /// string for expressions and out-of-range columns.
    pub fn column_table_name(&self, i: i64) -> String {
        if i < 0 {
            return String::new();
        }
        unsafe {
            let value = ffi::sqlite3_column_table_name(self.ptr, i as c_int);
            if value.is_null() {
                String::new()
            } else {
                CStr::from_ptr(value).to_string_lossy().into_owned()
            }
        }
    }

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

    /// Binds a text value (copied via `SQLITE_TRANSIENT`) to placeholder `idx`,
    /// using the caller-supplied `len` (the value's true byte length) rather than
    /// SQLite's strlen-based `-1` sentinel, so a value with an embedded NUL byte
    /// binds in full instead of truncating at the first NUL. A null pointer binds
    /// SQL NULL. A non-positive or `c_int`-overflowing `len` is treated as a
    /// zero-length string rather than being cast as-is, matching `bind_blob`'s
    /// clamp. Returns `1`/`0`.
    ///
    /// # Safety
    /// `val`, when non-null, must point to at least `len` readable bytes valid for
    /// the call.
    pub unsafe fn bind_text(&self, idx: i64, val: *const c_char, len: i64) -> i64 {
        if val.is_null() {
            return (ffi::sqlite3_bind_null(self.ptr, idx as c_int) == ffi::SQLITE_OK) as i64;
        }
        let safe_len = if len <= 0 || len > c_int::MAX as i64 {
            0
        } else {
            len as c_int
        };
        let rc = ffi::sqlite3_bind_text(
            self.ptr,
            idx as c_int,
            val,
            safe_len,
            ffi::SQLITE_TRANSIENT(),
        );
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

    /// Finalizes the statement, releasing the native handle. Idempotent: a second
    /// call — or the `Drop` net — is a no-op. `sqlite3_finalize` destroys the
    /// statement whatever it returns (its result code reports the *last step's*
    /// error, not a failure to free), so one call always releases.
    pub fn finalize(&self) {
        if self.released.replace(true) || self.ptr.is_null() {
            return;
        }
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

    /// Returns whether SQLite has stepped this statement and not yet reset or finalized it.
    pub fn busy(&self) -> i64 {
        unsafe { (ffi::sqlite3_stmt_busy(self.ptr) != 0) as i64 }
    }

    /// Returns SQLite's current explain mode for this statement.
    pub fn explain_mode(&self) -> i64 {
        unsafe { ffi::sqlite3_stmt_isexplain(self.ptr) as i64 }
    }

    /// Selects SQLite's prepared, EXPLAIN, or EXPLAIN QUERY PLAN mode.
    pub fn set_explain_mode(&self, mode: i64) -> i64 {
        if !(0..=2).contains(&mode) {
            return 0;
        }
        unsafe { (ffi::sqlite3_stmt_explain(self.ptr, mode as c_int) == ffi::SQLITE_OK) as i64 }
    }
}
