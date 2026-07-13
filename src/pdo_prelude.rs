//! Purpose:
//! The PDO standard-library surface (SQLite + PostgreSQL + MySQL/MariaDB drivers),
//! implemented in elephc-PHP. Declares the driver-agnostic `elephc_pdo` bridge
//! externs and the `PDO`, `PDOStatement`, and `PDOException` classes, so the whole
//! feature compiles through the normal pipeline (classes, methods, exceptions,
//! mixed arrays, C-ABI extern calls) instead of bespoke intrinsics and assembly.
//! The bridge dispatches to the right driver from the DSN prefix (`sqlite:` /
//! `pgsql:` / `mysql:`), so the same prelude serves every database.
//!
//! Called from:
//! - `crate::pipeline::compile()` and the codegen test harness via `inject_if_used`,
//!   after include resolution and before name resolution.
//!
//! Key details:
//! - The prelude is only injected when the program references PDO, so non-PDO
//!   binaries never declare the `elephc_pdo` externs and therefore never link
//!   `-lelephc_pdo`.
//! - The prelude carries only declarations (extern block + classes), which are
//!   discovered position-independently, so it is prepended to user code without
//!   changing top-level execution order.
//! - Method-local variables are `$_`-prefixed because the checker resolves a
//!   method-body variable's type against top-level variables of the same name; a
//!   user global like `$stmt` (a `PDOStatement`) would otherwise clash with a
//!   plain method-local `$stmt`. The `$_` prefix also exempts them from the
//!   unused-variable warning.

use crate::parser::ast::Program;

mod detect;

/// The elephc-PHP source implementing PDO over the driver-agnostic `elephc_pdo`
/// bridge (SQLite + PostgreSQL + MySQL/MariaDB).
///
/// Fetch-mode integers match PHP (`FETCH_ASSOC`=2, `FETCH_NUM`=3, `FETCH_BOTH`=4,
/// `FETCH_OBJ`=5); the bridge reports SQLite-compatible column-type integers for
/// both drivers (1=INTEGER, 2=FLOAT, 3=TEXT, 4=BLOB, 5=NULL). Method-default
/// literals use the numeric values directly to avoid const-in-default-value
/// evaluation edge cases.
pub const PDO_PRELUDE_SRC: &str = r#"<?php

extern "elephc_pdo" {
    function elephc_pdo_open(string $dsn): int;
    // v17 adds $sqlite_flags: the raw sqlite3_open_v2 flags for a `sqlite:` DSN
    // (0 = default READWRITE|CREATE), ignored for pgsql:/mysql: DSNs. Backs
    // Pdo\Sqlite::ATTR_OPEN_FLAGS (P1-10); a `file:` DSN body always gets
    // SQLITE_OPEN_URI OR-ed in bridge-side regardless of this value (P2-9).
    // v18 adds $my_init_command: one SQL statement run right after
    // authentication on a `mysql:` connection ("" = none), ignored for
    // sqlite:/pgsql: DSNs. Backs the minimal wiring for
    // Pdo\Mysql::ATTR_INIT_COMMAND (P1-9). $my_ssl_config (v19) is the packed
    // Pdo\Mysql::ATTR_SSL_* options ("ca=...;cert=...;key=...;verify=0|1", "" = no
    // TLS), applied to the mysql: rustls backend (requires the opt-in `mysql-tls`
    // build feature); ignored for sqlite:/pgsql: DSNs — PostgreSQL carries its own
    // sslmode/sslrootcert in the DSN and needs no extra parameter.
    // v25 adds the last two parameters:
    // - $my_found_rows (F-MY-06): 1 when Pdo\Mysql::ATTR_FOUND_ROWS was set truthy in
    //   the constructor's $options, which makes the bridge negotiate
    //   CLIENT_FOUND_ROWS in the handshake so an UPDATE's rowCount() reports MATCHED
    //   rather than CHANGED rows (php-src mysql_driver.c:776-778). Ignored for
    //   sqlite:/pgsql: DSNs.
    // - $persistent_key (F-CORE-16): the user-supplied ATTR_PERSISTENT key string,
    //   which joins the DSN in the persistent pool's hash key exactly as php-src's
    //   pdo_dbh.c:389-404 does ("" = the plain boolean-persistent pool). Two
    //   persistent connections to the SAME DSN under DIFFERENT key strings are
    //   therefore distinct pooled entries, which is the whole point of the key.
    function elephc_pdo_open_persistent(string $dsn, int $persistent, int $sqlite_flags, string $my_init_command, string $my_ssl_config, int $my_found_rows, string $persistent_key): int;
    function elephc_pdo_last_open_error(): string;
    function elephc_pdo_close(int $conn): void;
    function elephc_pdo_exec(int $conn, string $sql): int;
    function elephc_pdo_last_insert_id(int $conn, string $name): int;
    function elephc_pdo_changes(int $conn): int;
    function elephc_pdo_begin(int $conn): int;
    function elephc_pdo_commit(int $conn): int;
    function elephc_pdo_rollback(int $conn): int;
    function elephc_pdo_errcode(int $conn): int;
    function elephc_pdo_errmsg(int $conn): string;
    function elephc_pdo_prepare(int $conn, string $sql): int;
    function elephc_pdo_bind_parameter_index(int $stmt, string $name): int;
    function elephc_pdo_bind_int(int $stmt, int $idx, int $val): int;
    function elephc_pdo_bind_double(int $stmt, int $idx, float $val): int;
    // v20 adds an explicit $len (the value's true byte length) to bind_text, so a
    // value with an embedded NUL byte binds in full instead of truncating at the
    // first NUL, and declares bind_blob (bridge-side since v7, but never called
    // from the prelude until now) so PDO::PARAM_LOB binds route to it.
    function elephc_pdo_bind_text(int $stmt, int $idx, string $val, int $len): int;
    function elephc_pdo_bind_blob(int $stmt, int $idx, string $data, int $len): int;
    function elephc_pdo_bind_null(int $stmt, int $idx): int;
    function elephc_pdo_reset(int $stmt): int;
    function elephc_pdo_clear_bindings(int $stmt): int;
    function elephc_pdo_step(int $stmt): int;
    function elephc_pdo_column_count(int $stmt): int;
    function elephc_pdo_column_name(int $stmt, int $i): string;
    function elephc_pdo_column_type(int $stmt, int $i): int;
    function elephc_pdo_column_int(int $stmt, int $i): int;
    function elephc_pdo_column_double(int $stmt, int $i): float;
    // column_data_len/column_data_ptr are the length-counted TEXT/BLOB accessors
    // every fetch path goes through (columnValue()): the bytes are handed over as a
    // (pointer, length) pair copied in one go with ptr_read_string, so embedded NUL
    // bytes survive. v24 REMOVED the NUL-terminated `elephc_pdo_column_text` extern
    // that used to sit here (F-QUAL-03): it was dead code whose bridge side ran the
    // value through store_cstr, silently truncating at the first NUL — a trap for
    // whoever reached for the "obvious" text accessor. column_data_byte reads a
    // single byte and is kept as the compat/fallback path.
    function elephc_pdo_column_data_len(int $stmt, int $i): int;
    function elephc_pdo_column_data_ptr(int $stmt, int $i): ptr;
    function elephc_pdo_column_data_byte(int $stmt, int $i, int $offset): int;
    function elephc_pdo_finalize(int $stmt): int;
    function elephc_pdo_driver_name(int $conn): string;
    // ABI v7 additions. SQLSTATE (W1) is per-connection and per-statement; the
    // statement error trio mirrors the connection-level errcode/errmsg/sqlstate.
    // set_busy_timeout/server_version back ATTR_TIMEOUT/ATTR_SERVER_VERSION (W5),
    // bind_bool binds a real boolean per driver (W5), and last_insert_id_text
    // renders a sequence id as text so oversized PostgreSQL values never truncate.
    function elephc_pdo_sqlstate(int $conn): string;
    function elephc_pdo_stmt_errcode(int $stmt): int;
    function elephc_pdo_stmt_errmsg(int $stmt): string;
    function elephc_pdo_stmt_sqlstate(int $stmt): string;
    function elephc_pdo_bind_bool(int $stmt, int $idx, int $val): int;
    function elephc_pdo_set_busy_timeout(int $conn, int $ms): int;
    function elephc_pdo_server_version(int $conn): string;
    function elephc_pdo_last_insert_id_text(int $conn, string $name): string;
    // v8: driver-specific accessors. backend_pid backs Pdo\Pgsql::getPid();
    // warning_count backs Pdo\Mysql::getWarningCount(). Each returns 0 for a
    // connection of a different driver.
    function elephc_pdo_backend_pid(int $conn): int;
    function elephc_pdo_warning_count(int $conn): int;
    // v9: PostgreSQL large objects + COPY. lob_create returns the new OID as text
    // (empty on error); copy_out returns the raw COPY TO STDOUT text.
    function elephc_pdo_lob_create(int $conn): string;
    function elephc_pdo_lob_unlink(int $conn, string $oid): int;
    function elephc_pdo_copy_in(int $conn, string $copy_sql, string $data): int;
    function elephc_pdo_copy_out(int $conn, string $copy_sql): string;
    // v10: SQLite column declared-type (for getColumnMeta native_type) + extension
    // loading. column_decltype is empty for a non-SQLite/expression column.
    function elephc_pdo_column_decltype(int $stmt, int $i): string;
    function elephc_pdo_load_extension(int $conn, string $path): int;
    // v11: PostgreSQL LISTEN/NOTIFY poll — returns `channel\tpid\tpayload`, empty if
    // none within the timeout.
    function elephc_pdo_get_notify(int $conn, int $timeout_ms): string;
    // v12: whole-BLOB / whole-large-object read (read-whole streams). blob_read
    // (SQLite) and lob_get (PostgreSQL) load the whole value into a shared buffer and
    // return its byte length (-1 on error); blob_byte reads one byte out of that
    // buffer. Since v24 the buffer is copied out in a single ptr_read_string through
    // blob_data_ptr (below) rather than drained a byte at a time, so blob_byte is now
    // only the fallback/compat accessor — both paths preserve embedded NUL bytes.
    function elephc_pdo_blob_read(int $conn, string $table, string $column, int $rowid, string $dbname): int;
    function elephc_pdo_lob_get(int $conn, string $oid): int;
    function elephc_pdo_blob_byte(int $offset): int;
    // v13: custom SQLite collation registration (Pdo\Sqlite::createCollation). The
    // callable is decomposed at the PHP layer into its descriptor pointer and the
    // shared codegen collation adapter address, so this extern takes two plain `ptr`
    // args and never a `callable`. Returns 1 on success, 0 on error.
    function elephc_pdo_create_collation(int $conn, string $name, ptr $descriptor, ptr $adapter): int;
    // v14: custom SQLite scalar function registration (Pdo\Sqlite::createFunction).
    // Same decompose-at-PHP shape as create_collation; `num_args` is the declared arity
    // (-1 = variadic) and `flags` carries the SQLITE_DETERMINISTIC bit. Returns 1 on
    // success, 0 on error.
    function elephc_pdo_create_function(int $conn, string $name, int $num_args, int $flags, ptr $descriptor, ptr $adapter): int;
    // v15: custom SQLite aggregate registration (Pdo\Sqlite::createAggregate). The step
    // and finalize callables are each decomposed into a descriptor pointer + shared
    // codegen adapter address, so this extern takes four plain `ptr` args and never a
    // `callable`. `num_args` is the declared arity (-1 = variadic). Returns 1 on
    // success, 0 on error.
    function elephc_pdo_create_aggregate(int $conn, string $name, int $num_args, ptr $step_descriptor, ptr $step_adapter, ptr $final_descriptor, ptr $final_adapter): int;
    // v16: drain one buffered PostgreSQL server NOTICE message
    // (Pdo\Pgsql::setNoticeCallback). Returns the message text, or an empty string
    // when none is pending. The prelude polls this after each exec()/query().
    function elephc_pdo_get_notice(int $conn): string;
    // v17: a live sqlite3_stmt_readonly() read for a SQLite statement (0 for a
    // non-SQLite or unknown handle). Backs
    // PDOStatement::getAttribute(Pdo\Sqlite::ATTR_READONLY_STATEMENT).
    function elephc_pdo_stmt_readonly(int $stmt): int;
    // v21: a live sql_mode read for a mysql: connection — is NO_BACKSLASH_ESCAPES
    // active in the current session (1) or not (0)? 0 for a non-MySQL or unknown
    // handle. Backs PDO::quote()'s MySQL branch (P1-f): under that mode backslash
    // is a literal character in a string literal, so the usual
    // backslash-escaping is unsafe (an escaped quote does not actually escape)
    // and must fall back to ''-doubling only, matching mysqlnd's own behavior.
    function elephc_pdo_no_backslash_escapes(int $conn): int;
    // v22: a live transaction-state read backing PDO::inTransaction() /
    // beginTransaction()'s already-active guard (P1-g). Returns 1/0 for SQLite
    // (sqlite3_get_autocommit, live); -1 ("unknown — use the caller's own
    // $inTxn flag") for PostgreSQL and MySQL/MariaDB, since neither client
    // crate this bridge uses exposes a public live transaction-status accessor.
    function elephc_pdo_in_transaction(int $conn): int;
    // v23: per-column PostgreSQL type metadata for getColumnMeta (P2-k). Both are
    // read off the prepared statement's column descriptors, so they are valid
    // regardless of the current row and describe the DECLARED column type rather
    // than a NULL cell's runtime storage class. native_type is the server's
    // pg_type.typname ("int4"/"bool"/"bytea"/…), empty for a non-pgsql or
    // out-of-range column; type_oid is the PQftype OID (0 for the same cases).
    // The prelude keys the pg branch off a non-zero OID and derives pdo_type from
    // it, mirroring php-src pdo_pgsql's PARAM_* switch. Empty/0 make SQLite and
    // MySQL fall through to the generic storage-class metadata unchanged.
    function elephc_pdo_column_native_type(int $stmt, int $i): string;
    function elephc_pdo_column_type_oid(int $stmt, int $i): int;
    // v24: bulk BLOB copy-out (F-QUAL-01). Points at the first byte of the shared
    // whole-BLOB / large-object buffer last filled by blob_read/lob_get, or NULL when
    // that buffer is empty. Same contract as column_data_ptr: valid only until the
    // next call that rewrites the cell, so the prelude copies it immediately with
    // ptr_read_string. Exists so blobStream() copies an N-byte value with ONE FFI call
    // instead of N calls to blob_byte (each of which locks the bridge's handle table).
    function elephc_pdo_blob_data_ptr(): ptr;
    // v24: sqlite3_extended_result_codes() (F-SQLT-02), backing
    // Pdo\Sqlite::ATTR_EXTENDED_RESULT_CODES (1002). With it on, the driver-specific
    // code in errorInfo[1] is the EXTENDED result code — SQLITE_CONSTRAINT_UNIQUE
    // (2067) rather than the plain SQLITE_CONSTRAINT (19) it degrades to otherwise.
    // Returns 1 on success, 0 for a non-SQLite or unknown handle.
    function elephc_pdo_set_extended_result_codes(int $conn, int $on): int;
    // v26: the rest of PostgreSQL's per-column metadata, completing getColumnMeta
    // (F-PG-01/F-PG-02). All three are read off the prepared statement's column
    // descriptors, so they describe the DECLARED column and are valid before any row
    // is fetched. Their neutral values for a non-pgsql statement are the SERVER'S OWN
    // neutral answers, not sentinels, which is why the prelude can emit them straight:
    // - table_oid = PQftable(): the OID of the table the column was selected FROM.
    //   0 is InvalidOid — the server's own answer for a column that is NOT a plain
    //   table column (an expression, a literal, an aggregate). php-src emits this key
    //   UNCONDITIONALLY, 0 included, so the prelude must too.
    // - len = PQfsize(): the type's BYTE WIDTH when it is fixed (int4 -> 4,
    //   timestamp -> 8, uuid -> 16), and -1 for a VARLENA (text/varchar/numeric/bytea/
    //   json/arrays). A VARCHAR(20) therefore reports len -1, NOT 20 — its declared 20
    //   surfaces through precision instead. That is real PDO, not an approximation.
    // - precision = PQfmod(): the RAW atttypmod, undecoded, exactly as php-src stores
    //   it — VARCHAR(20) is 24 (20 + VARHDRSZ), NUMERIC(10,2) is 655366
    //   (((10 << 16) | 2) + 4). Decoding it here would be a divergence dressed up as an
    //   improvement.
    // v26 ALSO widens elephc_pdo_column_native_type (declared with the v23 pair above)
    // to mysql: statements, which now report MySQL's own wire-type names ("LONG",
    // "VAR_STRING", "NEWDECIMAL", "BLOB", …) per php-src's type_to_name_native.
    function elephc_pdo_column_table_oid(int $stmt, int $i): int;
    function elephc_pdo_column_len(int $stmt, int $i): int;
    function elephc_pdo_column_precision(int $stmt, int $i): int;
}

// F-SURF-01: php-src's ext/pdo/pdo.stub.php declares a GLOBAL `pdo_drivers(): array`
// alongside the class surface — the procedural spelling of PDO::getAvailableDrivers(),
// and still the one most capability probes reach for
// (`in_array('pgsql', pdo_drivers(), true)`). It was absent here entirely, so such a
// probe failed to compile rather than reporting the drivers this build has.
//
// The list is duplicated from PDO::getAvailableDrivers() rather than delegated to it: a
// global function calling a static method is a dispatch shape this prelude uses nowhere
// else, and the value is a one-line literal. The two MUST be kept in lockstep.
//
// KNOWN GAP (prelude-injection, not this function): the prelude is only injected for a
// program that names a PDO class (src/pdo_prelude/detect.rs scans for PDO /
// PDOStatement / PDOException / Pdo\<driver>), so a program whose ONLY PDO reference is
// a bare `pdo_drivers()` call still needs `--with-pdo` to force injection. Teaching the
// detector this function name is a one-line change in that file.
function pdo_drivers(): array {
    return ["mysql", "pgsql", "sqlite"];
}

class PDOException extends RuntimeException {
    // PHP surfaces the [SQLSTATE, driver-specific code, message] triple here;
    // frameworks (Doctrine, Laravel) read $e->errorInfo[0] for the SQLSTATE. Typed
    // `?array` (not left untyped): an untyped property fed both an array literal (SQL
    // errors) and an explicit null (unrecognized-driver connect failure) reads back as
    // a corrupted Mixed — `$e->errorInfo === null` returns the wrong answer, `[0]` will
    // not index, and var_dump SIGSEGVs — because the Mixed slot loses its type tag
    // across the heterogeneous call sites. The explicit `?array` gives the checker one
    // coherent representation and keeps the null "no structured info" case (a
    // connection-open failure with no server-reported SQLSTATE).
    public ?array $errorInfo = null;

    // F-SURF-11: the previous exception in the chain. php-src keeps this in the base
    // Exception's PRIVATE `$previous` slot, reachable only through getPrevious(); elephc's
    // built-in Exception has no such slot at all (its getPrevious() is a compiler
    // intrinsic, see below), so the chain is stored — and read back — here instead.
    //
    // KNOWN DIVERGENCE (compiler limitation, NOT faked): `$e->getPrevious()` still
    // returns null. Every call to a Throwable "standard method" (getMessage/getCode/
    // getFile/getLine/getTrace/getTraceAsString/getPrevious/__toString) on any
    // Throwable-like receiver is intercepted in codegen BEFORE user-method dispatch
    // (src/codegen/lower_inst.rs, is_throwable_standard_method_call →
    // lower_throwable_standard_method) and getPrevious() is lowered to a hardcoded null
    // (lower_throwable_null_previous). A `public function getPrevious()` override
    // declared here would therefore be dead code — never dispatched — so none is
    // declared, and the chain is exposed through this public property instead:
    //   `$e->previous` (elephc)   ==   `$e->getPrevious()` (PHP).
    public ?Throwable $previous = null;

    // F-SURF-10/F-SURF-11 — divergences from PHP's inherited
    // `Exception(string $message, int $code, ?Throwable $previous)` signature, all of them
    // forced by elephc's type system:
    //
    //  * The 2nd parameter is `?array $errorInfo`, not `int $code`. php-src populates
    //    PDOException::$errorInfo from PDO internals (pdo_throw_exception) and never from
    //    the constructor, but elephc's prelude has no internal channel to reach a
    //    just-constructed object's property from the throw site, so the triple is passed in
    //    here. Making the slot polymorphic (`mixed $codeOrErrorInfo`) to accept BOTH shapes
    //    was rejected: an untyped/Mixed value flowing into the `?array $errorInfo` property
    //    is exactly the corrupting shape the property comment above documents. Consequence:
    //    `new PDOException($msg, $code, $prev)` — the inherited PHP form — is a type error;
    //    use `new PDOException($msg, $errorInfo, $prev)`.
    //  * php-src stores the SQLSTATE **string** in `$code` (pdo_throw_exception does
    //    `zend_update_property_string(..., "code", ..., *pdo_error)`; the stub types it
    //    `int|string`). elephc's base Exception `$code` is `protected int`
    //    (src/types/checker/builtin_types/exception.rs), so a 5-character SQLSTATE cannot
    //    live there. The SQLSTATE stays in `errorInfo[0]`, exactly as before — read
    //    `$e->errorInfo[0]` where PHP code reads `$e->getCode()`. What `$code` DOES carry
    //    now (it was never assigned before, so getCode() was a constant 0) is the
    //    driver-specific integer code, i.e. `errorInfo[1]` — the same integer php-src puts
    //    in `errorInfo[1]` and interpolates into the "SQLSTATE[%s] [%d] %s" message.
    public function __construct(string $message = "", ?array $errorInfo = null, ?Throwable $previous = null) {
        // The built-in Exception constructor is a checker-synthesized method with
        // no linkable symbol, so `parent::__construct()` cannot be called; the
        // public `$message` property (see getMessage()) is assigned directly.
        $this->message = $message;
        $this->errorInfo = $errorInfo;
        $this->previous = $previous;
        // F-SURF-10: populate the inherited `protected int $code` with the ONLY meaningful
        // integer this exception carries — the driver-specific error code. `$code` is a
        // real property slot of the built-in Throwable payload (the compiler's getCode()
        // intrinsic reads that same slot), so this assignment is what makes getCode()
        // report something other than 0. is_array() narrowing (not `!== null`) is used
        // because it is the guard shape the checker narrows most reliably here, and the
        // element is re-checked with is_int() because errorInfo[1] is null for a
        // connect-time failure with no server-reported code.
        if (is_array($errorInfo)) {
            if (count($errorInfo) > 1) {
                $_driverCode = $errorInfo[1];
                if (is_int($_driverCode)) {
                    $this->code = (int) $_driverCode;
                }
            }
        }
    }
}

class PDO {
    const FETCH_ASSOC = 2;
    const FETCH_NUM = 3;
    const FETCH_BOTH = 4;
    const FETCH_OBJ = 5;
    const FETCH_COLUMN = 7;
    const FETCH_CLASS = 8;
    const FETCH_INTO = 9;
    const PARAM_NULL = 0;
    const PARAM_INT = 1;
    const PARAM_STR = 2;
    const PARAM_BOOL = 5;
    const ATTR_TIMEOUT = 2;
    const ATTR_ERRMODE = 3;
    const ATTR_PERSISTENT = 12;
    const ATTR_DRIVER_NAME = 16;
    const ERRMODE_SILENT = 0;
    const ERRMODE_WARNING = 1;
    const ERRMODE_EXCEPTION = 2;
    const ERR_NONE = "00000";
    // Additional PHP 8.4 fetch-mode constants (base modes and OR-able flags).
    const FETCH_DEFAULT = 0;
    const FETCH_LAZY = 1;
    const FETCH_BOUND = 6;
    const FETCH_FUNC = 10;
    const FETCH_NAMED = 11;
    const FETCH_KEY_PAIR = 12;
    const FETCH_GROUP = 0x10000;
    const FETCH_UNIQUE = 0x30000;
    const FETCH_CLASSTYPE = 0x40000;
    const FETCH_SERIALIZE = 0x80000;
    const FETCH_PROPS_LATE = 0x100000;
    const FETCH_ORI_NEXT = 0;
    const FETCH_ORI_PRIOR = 1;
    const FETCH_ORI_FIRST = 2;
    const FETCH_ORI_LAST = 3;
    const FETCH_ORI_ABS = 4;
    const FETCH_ORI_REL = 5;
    // Parameter-type constants.
    const PARAM_LOB = 3;
    const PARAM_STMT = 4;
    const PARAM_INPUT_OUTPUT = 0x80000000;
    const PARAM_STR_NATL = 0x40000000;
    const PARAM_STR_CHAR = 0x20000000;
    // F-SURF-03: the parameter-lifecycle event constants. Their values are the
    // DECLARATION ORDER of `enum pdo_param_event` in php-src's
    // ext/pdo/php_pdo_driver.h, which is the only thing that fixes them (the enum
    // carries no explicit values). They exist for userspace/native PDO *driver*
    // authorship — a driver's `param_hook` is called once per event so it can
    // allocate, rewrite, or free a bound parameter around each stage of a
    // statement's life. elephc's bridge implements the drivers natively in Rust and
    // exposes no param-hook seam to PHP, so these constants are entirely INERT here:
    // they are declared purely so code that references PDO::PARAM_EVT_* (portable
    // driver shims, test suites enumerating the class surface) still compiles.
    const PARAM_EVT_ALLOC = 0;
    const PARAM_EVT_FREE = 1;
    const PARAM_EVT_EXEC_PRE = 2;
    const PARAM_EVT_EXEC_POST = 3;
    const PARAM_EVT_FETCH_PRE = 4;
    const PARAM_EVT_FETCH_POST = 5;
    const PARAM_EVT_NORMALIZE = 6;
    // Driver/connection attribute constants (PHP 8.4 numeric values).
    const ATTR_AUTOCOMMIT = 0;
    const ATTR_PREFETCH = 1;
    const ATTR_SERVER_VERSION = 4;
    const ATTR_CLIENT_VERSION = 5;
    const ATTR_SERVER_INFO = 6;
    const ATTR_CONNECTION_STATUS = 7;
    const ATTR_CASE = 8;
    const ATTR_CURSOR_NAME = 9;
    const ATTR_CURSOR = 10;
    const ATTR_ORACLE_NULLS = 11;
    const ATTR_STATEMENT_CLASS = 13;
    const ATTR_FETCH_TABLE_NAMES = 14;
    const ATTR_FETCH_CATALOG_NAMES = 15;
    const ATTR_STRINGIFY_FETCHES = 17;
    const ATTR_MAX_COLUMN_LEN = 18;
    const ATTR_DEFAULT_FETCH_MODE = 19;
    const ATTR_EMULATE_PREPARES = 20;
    const ATTR_DEFAULT_STR_PARAM = 21;
    const ATTR_DRIVER_SPECIFIC = 1000;
    // Column-case, null-handling, and cursor-orientation constants.
    const CASE_NATURAL = 0;
    const CASE_UPPER = 1;
    const CASE_LOWER = 2;
    const NULL_NATURAL = 0;
    const NULL_EMPTY_STRING = 1;
    const NULL_TO_STRING = 2;
    const CURSOR_FWDONLY = 0;
    const CURSOR_SCROLL = 1;
    // F-SQLT-01: php-src registers the SQLite driver constants on the BASE \PDO
    // class as well as on Pdo\Sqlite (ext/pdo_sqlite/pdo_sqlite.c registers them
    // against pdo_dbh_ce, in parallel with the modern class-scoped spellings added
    // in 8.1) — `PDO::SQLITE_ATTR_OPEN_FLAGS` and friends are the pre-8.1 API
    // surface a great deal of real-world code still uses. Same values as the
    // Pdo\Sqlite constants further down; the two spellings are aliases, both live.
    const SQLITE_DETERMINISTIC = 2048;
    const SQLITE_ATTR_OPEN_FLAGS = 1000;
    const SQLITE_OPEN_READONLY = 1;
    const SQLITE_OPEN_READWRITE = 2;
    const SQLITE_OPEN_CREATE = 4;
    const SQLITE_ATTR_READONLY_STATEMENT = 1001;
    const SQLITE_ATTR_EXTENDED_RESULT_CODES = 1002;

    private int $conn;
    private int $errMode;
    private bool $persistent;
    private array $attributes;
    private bool $inTxn;
    private int $defaultFetchMode;
    // P1-11 (best-effort): ATTR_STRINGIFY_FETCHES, threaded to each statement at
    // prepare() time the same way $defaultFetchMode already is. This is a
    // snapshot, not a live read of the connection's current value — a divergence
    // already accepted for $defaultFetchMode, so a setAttribute() call after a
    // statement is prepared does not retroactively affect it (real PHP re-checks
    // the connection attribute on every fetch).
    private bool $stringifyFetches;
    // P2-e: ATTR_CASE (folds fetched column-name keys) and ATTR_ORACLE_NULLS
    // (folds NULL<->"" in fetched scalar values), both threaded to each
    // statement at prepare()/query() time the same way $defaultFetchMode /
    // $stringifyFetches already are — a prepare()-time snapshot, not a live
    // read of the connection's current value (the same accepted divergence).
    private int $attrCase;
    private int $oracleNulls;

    // F-CORE-11: php-src supports an INDIRECT DSN — `new PDO("uri:<url>")` reads the
    // real DSN from the FIRST LINE of the referenced stream (`dsn_from_uri`,
    // pdo_dbh.c:208-220, called from the constructor at pdo_dbh.c:346-358, ahead of the
    // driver lookup), so a credentials-bearing DSN can live outside the source tree.
    // This prelude had no `uri:` handling at all, so such a DSN reached the bridge
    // verbatim and failed as an unknown driver. Returns the DSN unchanged when it
    // carries no `uri:` prefix, so every caller pipes its raw argument through this
    // unconditionally (and re-running it on an already-resolved DSN is a no-op, which is
    // what lets the driver subclasses resolve first and still hand the result to
    // parent::__construct()).
    //
    // Two divergences, both forced by elephc's I/O surface rather than chosen:
    // (1) php-src opens the URI through the full stream-wrapper stack. elephc's fopen()
    //     has no `file://` wrapper (its wrapper table — src/codegen/lower_inst/builtins/
    //     io.rs — covers php://, data://, ftp://, phar://, http://, compress.*:// and
    //     nothing else), so the `file://` scheme — the very one PHP's own documentation
    //     uses for this feature — is stripped here and the remainder opened as a plain
    //     path. Any other scheme is handed to fopen() as-is and simply fails to open,
    //     which lands on the same error below.
    // (2) php-src's php_stream_get_line KEEPS the trailing newline; it is trimmed here,
    //     since a DSN carrying a stray "\n" reaches the driver parsers as a garbage
    //     trailing key.
    //
    // (3) php-src DEPRECATED this whole DSN form ("Looking up the DSN from a URI is
    //     deprecated due to possible security concerns with DSNs coming from remote
    //     URIs") and emits an E_DEPRECATED alongside the successful lookup. elephc has no
    //     deprecation-diagnostic channel, so the notice is documented here instead of
    //     raised; the feature still works, exactly as it still does in PHP.
    //
    // The two failure messages and their EXCEPTION CLASS were verified against a real
    // PHP 8.5.6 CLI rather than read off the C source, because php-src raises them with
    // `zend_argument_error(pdo_exception_ce, 1, …)` — an ARGUMENT-ERROR MESSAGE SHAPE
    // ("…(): Argument #1 ($dsn) must be …") thrown as a **PDOException**, NOT as a
    // ValueError. Reading only the `zend_argument_*` call would have produced the wrong
    // class here:
    //   unreadable URI / empty first line -> "…must be a valid data source URI"
    //   first line with no colon in it    -> "…must be a valid data source name (via URI)"
    protected function resolveDsnUri(string $dsn): string {
        if (!str_starts_with($dsn, "uri:")) {
            return $dsn . "";
        }
        $_uri = substr($dsn, 4);
        if (str_starts_with($_uri, "file://")) {
            $_uri = substr($_uri, 7);
        }
        $_uriHandle = fopen($_uri, "rb");
        if ($_uriHandle === false) {
            throw new PDOException("PDO::__construct(): Argument #1 (\$dsn) must be a valid data source URI", null);
        }
        $_uriLine = fgets($_uriHandle);
        fclose($_uriHandle);
        if ($_uriLine === false) {
            // EOF on the very first read: the stream opened but is empty, which php-src
            // reports identically to an unopenable one (dsn_from_uri returns NULL for both).
            throw new PDOException("PDO::__construct(): Argument #1 (\$dsn) must be a valid data source URI", null);
        }
        // Explicit cast: the checker does not narrow fgets()'s `string|false` out of the
        // `=== false` guard above (the same accepted gap copyFromFile() casts around for
        // file_get_contents).
        $_resolved = rtrim((string) $_uriLine, "\r\n");
        if (strpos($_resolved, ":") === false) {
            throw new PDOException("PDO::__construct(): Argument #1 (\$dsn) must be a valid data source name (via URI)", null);
        }
        return $_resolved;
    }

    // F-CORE-13: php-src validates the DSN in two steps, with two DIFFERENT messages,
    // both before any driver is asked to connect (pdo_dbh.c:346-372):
    //   1. no colon at all -> the ARGUMENT-ERROR message shape
    //      "PDO::__construct(): Argument #1 ($dsn) must be a valid data source name";
    //   2. a colon but no driver registered for the prefix -> the BARE message
    //      "could not find driver" (php-src deliberately keeps the DSN out of that text:
    //      it may carry a password).
    // Neither existed here: the constructor let the bridge fail the open and surfaced ITS
    // message, "could not find driver (only sqlite:, pgsql:, and mysql: DSNs are
    // supported)", while PDO::connect() threw php-src's bare text — two different messages
    // for one failure inside one class, and a colonless DSN got neither. The helpful
    // driver list survives in this comment and in docs/php/pdo.md rather than in an
    // exception message callers may match on.
    //
    // BOTH are PDOExceptions — VERIFIED against a real PHP 8.5.6 CLI, and worth stating
    // because the C source misleads: case 1 is raised with
    // `zend_argument_error(pdo_exception_ce, 1, …)`, whose first parameter is the
    // exception class entry, so it produces an argument-error MESSAGE SHAPE thrown as a
    // **PDOException** — NOT a ValueError, despite reading like every other
    // zend_argument_* call site in the tree. `get_class($e)` on a real
    // `new PDO("nocolon")` is "PDOException".
    //
    // Divergence: the message names `PDO::__construct()` even when the call came through
    // a driver subclass (php-src names the called scope). elephc has no late static
    // binding — `static::` lowers to the DEFINING class (src/ir_lower/expr/mod.rs:9654) —
    // so the called scope is not observable from here.
    protected function checkDsnIsSupported(string $dsn): void {
        // The `null` second argument is passed EXPLICITLY at both throw sites (not left to
        // PDOException's default): a bare `new PDOException($msg)` omitting it does not
        // actually read back as `null` (a pre-existing, general
        // default-argument-materialization bug — reproducible with plain
        // `throw new PDOException("x")`, unrelated to PDO). php-src leaves errorInfo null
        // for both of these: no driver ever attempted a connection, so there is no
        // SQLSTATE to report.
        if (strpos($dsn, ":") === false) {
            throw new PDOException("PDO::__construct(): Argument #1 (\$dsn) must be a valid data source name", null);
        }
        if (!str_starts_with($dsn, "sqlite:") && !str_starts_with($dsn, "pgsql:") && !str_starts_with($dsn, "mysql:")) {
            throw new PDOException("could not find driver", null);
        }
    }

    // F-CORE-01: php-src refuses to open a FOREIGN DSN through a driver-specific
    // subclass — `create_driver_specific_pdo_object` (pdo_dbh.c:222-299) compares the
    // DSN's driver against the called scope and throws when they differ. elephc's three
    // subclasses forwarded blindly (and Pdo\Mysql had no constructor at all), so
    // `new Pdo\Sqlite("mysql:host=…")` happily returned a Pdo\Sqlite object holding a
    // live MySQL connection — an object whose class lies about what it is, and whose
    // SQLite-only methods (openBlob, createFunction, …) then fail deep in the bridge.
    //
    // Called from each subclass constructor BEFORE parent::__construct(), i.e. before
    // any connection attempt, which is where php-src runs it too. A DSN whose prefix is
    // no driver this bridge knows is deliberately NOT rejected here: that is a different
    // failure with a different message, owned by checkDsnIsSupported() a moment later.
    //
    // Divergence (unfixable without late static binding, see checkDsnIsSupported()):
    // php-src throws the same error for the STATIC form, `Pdo\Sqlite::connect("mysql:…")`,
    // with "connect()" swapped in for "__construct()". PDO::connect() is a plain
    // inherited static here and cannot see which subclass it was called through, so that
    // spelling still dispatches on the DSN prefix alone.
    protected function checkDriverSubclassDsn(string $dsn, string $calledClass, string $expectedDriver): void {
        if (str_starts_with($dsn, $expectedDriver . ":")) {
            return;
        }
        $_dsnDriver = "";
        $_dsnClass = "";
        if (str_starts_with($dsn, "sqlite:")) {
            $_dsnDriver = "sqlite";
            $_dsnClass = "Pdo\\Sqlite";
        } elseif (str_starts_with($dsn, "mysql:")) {
            $_dsnDriver = "mysql";
            $_dsnClass = "Pdo\\Mysql";
        } elseif (str_starts_with($dsn, "pgsql:")) {
            $_dsnDriver = "pgsql";
            $_dsnClass = "Pdo\\Pgsql";
        }
        if ($_dsnDriver === "") {
            return;
        }
        throw new PDOException($calledClass . "::__construct() cannot be used for connecting to the \"" . $_dsnDriver . "\" driver, either call " . $_dsnClass . "::__construct() or PDO::__construct() instead");
    }

    public function __construct(string $dsn, ?string $username = null, ?string $password = null, ?array $options = null) {
        // F-CORE-11 / F-CORE-13: resolve an indirect `uri:` DSN and validate the result
        // FIRST — php-src does both ahead of the options loop and the driver connect
        // (pdo_dbh.c:346-372). Every later DSN test in this method reads $_dsn, never the
        // raw $dsn parameter, which for a `uri:` DSN still says "uri:…".
        $_dsn = $this->resolveDsnUri($dsn);
        $this->checkDsnIsSupported($_dsn);
        $this->errMode = 2;
        $this->persistent = false;
        $this->attributes = [];
        $this->inTxn = false;
        $this->defaultFetchMode = 4;
        $this->stringifyFetches = false;
        $this->attrCase = 0;
        $this->oracleNulls = 0;
        // P1-10: Pdo\Sqlite::ATTR_OPEN_FLAGS, read from $options here and applied
        // at the open call below. Its numeric value (1000) is PDO_ATTR_DRIVER_SPECIFIC
        // (see self::ATTR_DRIVER_SPECIFIC) — the same value MySQL/PostgreSQL use for
        // their own first driver-specific attribute, but this is harmless: the bridge
        // only consults $_openFlags for a `sqlite:` DSN and ignores it otherwise.
        $_openFlags = 0;
        // P1-9: Pdo\Mysql::ATTR_INIT_COMMAND (minimal wiring — one SQL statement
        // run right after authentication), read from $options here and applied at
        // the open call below. Its numeric value (1002) collides with
        // Pdo\Sqlite::ATTR_EXTENDED_RESULT_CODES, harmlessly: the bridge only
        // consults $_myInitCommand for a `mysql:` DSN and ignores it otherwise.
        $_myInitCommand = "";
        // Pdo\Mysql::ATTR_SSL_* (1007/1008/1009/1014): read here into a packed
        // "ca=…;cert=…;key=…;verify=0|1" string ($_mySslConfig below) that the
        // bridge applies to the mysql: rustls TLS backend. These numeric values do
        // not collide with any sqlite:/pgsql: driver-specific constant, and the
        // bridge only consults $_mySslConfig for a mysql: DSN, so they stay inert
        // for the other drivers. ATTR_SSL_CAPATH (1010)/ATTR_SSL_CIPHER (1011) have
        // no rustls SslOpts equivalent and are intentionally not wired (stored in
        // $this->attributes only). $_mySslVerify stays -1 ("unset") until an
        // explicit ATTR_SSL_VERIFY_SERVER_CERT is seen.
        $_mySslCa = "";
        $_mySslCert = "";
        $_mySslKey = "";
        $_mySslVerify = -1;
        // F-MY-06: Pdo\Mysql::ATTR_FOUND_ROWS (1005), threaded to the bridge's connect
        // path below. F-CORE-16: the user-supplied ATTR_PERSISTENT pool key ("" = the
        // plain boolean-persistent pool). Both are read from $options in the loop below.
        $_myFoundRows = 0;
        $_persistentKey = "";
        // Constructor options affect the connection that is opened below, so
        // apply them before the bridge sees the DSN. In particular,
        // ATTR_PERSISTENT selects the bridge's process-local DSN pool.
        if ($options !== null) {
            foreach ($options as $_attr => $_val) {
                $_iattr = (int) $_attr;
                if ($_iattr == 3) {
                    // P1-h: same ATTR_ERRMODE value validation as setAttribute() below —
                    // a bad mode must not silently take effect via the constructor either.
                    // F-CORE-03: including the SHAPE check (attrIntValue), which php-src
                    // runs on the constructor's options array through the very same
                    // pdo_get_long_param() path — this loop had the identical blind-cast
                    // hole, so `new PDO($dsn, null, null, [PDO::ATTR_ERRMODE => "banana"])`
                    // used to open the connection in ERRMODE_SILENT.
                    $_ctorErrMode = $this->attrIntValue($_val);
                    $this->checkErrMode($_ctorErrMode);
                    $this->errMode = $_ctorErrMode;
                } elseif ($_iattr == 12) {
                    // F-CORE-16: the CONSTRUCTOR's ATTR_PERSISTENT does NOT go through
                    // pdo_get_bool_param — pdo_dbh.c:389-404 special-cases it entirely, in
                    // two arms this branch mirrors one for one:
                    //   * a NON-NUMERIC, NON-EMPTY STRING is a user-supplied POOL KEY: the
                    //     connection is persistent AND that string joins the DSN in the
                    //     persistent pool's hash key, so two persistent connections to one
                    //     DSN under different keys stay DISTINCT handles (that separation is
                    //     the entire point of the named form);
                    //   * anything else is `is_persistent = zval_get_long(v) ? 1 : 0` — a
                    //     plain NUMERIC COERCION, so a numeric string, an empty string, a
                    //     float and a bool all just coerce, and NONE of them is an error.
                    // Both arms were wrong here: this used to call attrBoolValue(), which
                    // threw the pool key away AND raised a spurious TypeError for every
                    // string form. Verified against a real PHP 8.5.6 CLI:
                    // ATTR_PERSISTENT => "keyA" gives persistent true; => "0" and => "" both
                    // give false, with no error raised for either.
                    if (is_string($_val) && ((string) $_val) !== "" && !is_numeric((string) $_val)) {
                        $this->persistent = true;
                        $_persistentKey = (string) $_val;
                    } else {
                        $this->persistent = ((int) $_val) != 0;
                    }
                } elseif ($_iattr == 19) {
                    // P1-h: same ATTR_DEFAULT_FETCH_MODE validation as setAttribute() below.
                    $_ctorFetchMode = $this->attrIntValue($_val);
                    $this->checkDefaultFetchMode($_ctorFetchMode);
                    $this->defaultFetchMode = $_ctorFetchMode;
                } elseif ($_iattr == 17) {
                    $this->stringifyFetches = $this->attrBoolValue($_val);
                } elseif ($_iattr == 8) {
                    // P2-e: same ATTR_CASE value validation as setAttribute() below.
                    $_ctorCase = $this->attrIntValue($_val);
                    $this->checkAttrCase($_ctorCase);
                    $this->attrCase = $_ctorCase;
                } elseif ($_iattr == 11) {
                    $this->oracleNulls = $this->attrIntValue($_val);
                } elseif ($_iattr == 2) {
                    // F-CORE-03: ATTR_TIMEOUT is consumed further down from
                    // $this->attributes (it needs the DSN, then a live connection), but
                    // its value must be shape-checked at the same point setAttribute()
                    // checks it — the RAW value is what gets stored below, and every
                    // later read does a bare `(int)` on it. attrIntValue()'s only job at
                    // this call site is therefore to raise the TypeError; its normalized
                    // result is deliberately unused.
                    $_unusedTimeout = $this->attrIntValue($_val);
                } elseif ($_iattr == 1000) {
                    $_openFlags = (int) $_val;
                } elseif ($_iattr == 1002) {
                    $_myInitCommand = (string) $_val;
                } elseif ($_iattr == 1005) {
                    // F-MY-06: Pdo\Mysql::ATTR_FOUND_ROWS. The value is 1005, NOT 1013
                    // (which is ATTR_MULTI_STATEMENTS): under mysqlnd — PHP's default, and
                    // the build this prelude's constant block mirrors — php-src's
                    // php_pdo_mysql_int.h enum omits MAX_BUFFER_SIZE/READ_DEFAULT_FILE/
                    // READ_DEFAULT_GROUP, so ATTR_COMPRESS=1003, ATTR_DIRECT_QUERY=1004 and
                    // ATTR_FOUND_ROWS=1005. Threaded to the bridge's connect path, which
                    // ORs CLIENT_FOUND_ROWS into the handshake capability flags
                    // (mysql_driver.c:776-778) so an UPDATE's rowCount() reports the number
                    // of rows MATCHED rather than the number actually CHANGED — the
                    // difference between 1 and 0 for an UPDATE writing the value a row
                    // already holds. No sqlite:/pgsql: constant shares this number, and the
                    // bridge only consults it for a mysql: DSN, so it is inert elsewhere.
                    $_myFoundRows = ((bool) $_val) ? 1 : 0;
                } elseif ($_iattr == 1009) {
                    $_mySslCa = (string) $_val;
                } elseif ($_iattr == 1008) {
                    $_mySslCert = (string) $_val;
                } elseif ($_iattr == 1007) {
                    $_mySslKey = (string) $_val;
                } elseif ($_iattr == 1014) {
                    $_mySslVerify = ((bool) $_val) ? 1 : 0;
                }
                $this->attributes[$_iattr] = $_val;
            }
        }
        // SQLite ignores credentials. For PostgreSQL and MySQL, the user/password may be
        // passed as the PDO constructor arguments (PHP-style); fold them into the DSN's
        // `key=value` list, where the bridge parses them.
        //
        // F-CORE-02: php-src's CREDENTIAL PRECEDENCE IS ASYMMETRIC BY DRIVER, and this
        // prelude used to apply the pgsql rule to both:
        //   pgsql (pgsql_driver.c:1377-1378) — the conninfo string is assembled with the
        //     DSN's own keys AFTER the constructor's user/password, and libpq's conninfo
        //     parsing is last-wins, so the DSN WINS. (P2-6 already implemented this, and
        //     it is correct: only a key the DSN does not carry is appended.)
        //   mysql (mysql_driver.c:948-953) — `if (!dbh->username && vars[5].optval)
        //     dbh->username = …` (same shape for the password): the DSN key is consulted
        //     ONLY as a fallback for an absent constructor argument, so the CONSTRUCTOR
        //     ARGUMENT WINS. `new PDO("mysql:host=h;user=readonly", "admin", $pw)`
        //     connects as `admin` in real PHP and used to connect as `readonly` here — a
        //     silent privilege swap in whichever direction the caller did not expect.
        //
        // MECHANISM (verified by reading the bridge parser, not assumed): a plain APPEND
        // is enough to make the constructor argument win for mysql, because
        // crates/elephc-pdo/src/my.rs::build_opts walks `body.split(';')` and assigns
        // `match key { "user" => user = Some(value), … }` into ONE slot per key — a later
        // duplicate simply overwrites the earlier one, i.e. the parser is LAST-WINS. The
        // DSN's own `user=`/`password=` therefore does not have to be stripped out.
        //
        // F-CORE-02 (follow-up): the LAST-WINS mechanism above still relies on the same
        // `body.split(';')` the DSN itself is scanned with, so a ';' embedded in the
        // constructor username/password would silently truncate the credential right
        // there (and a stray '%' would collide with the percent-decoding this note is
        // about to describe). Percent-encode '%' and ';' on the credential VALUE before
        // appending it — '%' FIRST, so the '%' introduced by encoding ';' is not itself
        // re-encoded — and percent-decode ONLY the user/password values on the bridge
        // side (my.rs/pg.rs). '=' needs no encoding since the parser splits on the first
        // '=' only. This leaves the ';'-splitter itself, and every non-credential value
        // (host, dbname with '\' or '%', etc.), byte-identical; a credential with no
        // special characters round-trips unchanged too.
        if (str_starts_with($_dsn, "pgsql:") || str_starts_with($_dsn, "mysql:")) {
            $_dsnIsMysql = str_starts_with($_dsn, "mysql:");
            if ($username !== null && ($_dsnIsMysql || !str_contains($_dsn, "user="))) {
                $_encUser = str_replace(";", "%3B", str_replace("%", "%25", $username));
                $_dsn = $_dsn . ";user=" . $_encUser;
            }
            if ($password !== null && ($_dsnIsMysql || !str_contains($_dsn, "password="))) {
                $_encPass = str_replace(";", "%3B", str_replace("%", "%25", $password));
                $_dsn = $_dsn . ";password=" . $_encPass;
            }
            // P2-1: ATTR_TIMEOUT maps to the driver's connect-time socket
            // timeout. libpq's `connect_timeout` conninfo key and the mysql
            // client's `connect_timeout` DSN key (mapped to
            // OptsBuilder::tcp_connect_timeout in my.rs) are both plain
            // `key=value` pairs their respective parsers already understand, so
            // folding this into the DSN needs no further bridge change — only
            // applied when the DSN does not already specify it.
            if (isset($this->attributes[2]) && !str_contains($_dsn, "connect_timeout=")) {
                $_dsn = $_dsn . ";connect_timeout=" . ((int) $this->attributes[2]);
            }
        }
        // Serialize the collected Pdo\Mysql::ATTR_SSL_* options into the packed
        // string the bridge parses (only the keys that were actually set are
        // emitted; an all-unset config stays "" = no TLS). File paths do not
        // contain ';'/'=' in practice, matching the rest of the bridge's DSN-style
        // parsing.
        $_mySslConfig = "";
        if ($_mySslCa !== "") {
            $_mySslConfig = $_mySslConfig . "ca=" . $_mySslCa . ";";
        }
        if ($_mySslCert !== "") {
            $_mySslConfig = $_mySslConfig . "cert=" . $_mySslCert . ";";
        }
        if ($_mySslKey !== "") {
            $_mySslConfig = $_mySslConfig . "key=" . $_mySslKey . ";";
        }
        if ($_mySslVerify != -1) {
            $_mySslConfig = $_mySslConfig . "verify=" . $_mySslVerify . ";";
        }
        $this->conn = elephc_pdo_open_persistent($_dsn, $this->persistent ? 1 : 0, $_openFlags, $_myInitCommand, $_mySslConfig, $_myFoundRows, $_persistentKey);
        if ($this->conn < 0) {
            $_openMsg = elephc_pdo_last_open_error();
            // P1-4: when a real driver recognized the DSN but the connection
            // itself failed (bad path / unreachable host / auth failure), PHP
            // prefixes the message "SQLSTATE[<state>]: ..." and populates a
            // 3-element errorInfo so the standard try/catch-around-`new PDO`
            // classification idiom (`$e->errorInfo[0]`) works. There is no live
            // connection yet to ask for a native SQLSTATE, so fall back to the
            // same class real PHP drivers default to for a connect-time failure:
            // "08006" (SQLSTATE connection-exception) for the network-facing
            // pgsql/mysql drivers, "HY000" (generic error — pdo_sqlite's own
            // default) otherwise; native code is unknown here (null).
            //
            // F-CORE-13: an UNRECOGNIZED DSN can no longer reach this point at all —
            // checkDsnIsSupported(), at the top of this constructor, already rejected it
            // with php-src's bare "could not find driver" (no SQLSTATE prefix, errorInfo
            // left null) before the bridge was ever called. So every failure here is a
            // genuine connect failure of a known driver and always carries a SQLSTATE;
            // the old prefix re-test and its bare-message fallback are gone with it.
            $_sqlstate = str_starts_with($_dsn, "sqlite:") ? "HY000" : "08006";
            throw new PDOException("SQLSTATE[" . $_sqlstate . "]: " . $_openMsg, [$_sqlstate, null, $_openMsg]);
        }
        // ATTR_TIMEOUT needs a live connection, so apply it after the open (the
        // pre-open loop only records it). PHP's value is in seconds; SQLite's
        // busy-timeout is milliseconds. For PostgreSQL/MySQL this is now a
        // harmless no-op layered on top of the connect_timeout DSN key above,
        // which is what actually bounds the connect-time wait (P2-1).
        if (isset($this->attributes[2])) {
            elephc_pdo_set_busy_timeout($this->conn, ((int) $this->attributes[2]) * 1000);
        }
    }

    private function fail(string $message): void {
        // Apply the current error mode to a failed operation. EXCEPTION throws;
        // WARNING writes to stderr and lets the caller return its failure value;
        // SILENT is quiet and the caller returns its failure value. The SQLSTATE
        // and native driver code are attached so callers can read $e->errorInfo
        // (frameworks parse errorInfo[0] as the SQLSTATE).
        if ($this->errMode == 0) {
            return;
        }
        $_sqlstate = elephc_pdo_sqlstate($this->conn);
        if ($this->errMode == 2) {
            $_native = elephc_pdo_errcode($this->conn);
            throw new PDOException("SQLSTATE[" . $_sqlstate . "]: " . $message, [$_sqlstate, $_native, $message]);
        }
        fwrite(STDERR, "PDO error: SQLSTATE[" . $_sqlstate . "]: " . $message . "\n");
    }

    // F-CORE-04/F-CORE-05: a SYNTHETIC (non-driver) connection-level error, mirroring
    // php-src's `pdo_raise_impl_error` — it writes a caller-given SQLSTATE instead of
    // reading the driver's live error state, because there was no failed query to read
    // one from. Fully errMode-aware, exactly like fail() above: EXCEPTION throws,
    // WARNING writes to stderr, SILENT is quiet — and in every mode the caller goes on
    // to return its own failure value. PDOStatement has carried the identical helper
    // since P1-i; \PDO was missing it, which is precisely why getAttribute() used to
    // answer a nonsense attribute number with a bare null instead of raising IM001.
    // (setAttribute() deliberately does NOT use it — see its own F-CORE-04 comment: real
    // PHP rejects an unknown attribute there SILENTLY.)
    private function failCode(string $sqlstate, string $message): void {
        if ($this->errMode == 0) {
            return;
        }
        if ($this->errMode == 2) {
            throw new PDOException("SQLSTATE[" . $sqlstate . "]: " . $message, [$sqlstate, null, $message]);
        }
        fwrite(STDERR, "PDO error: SQLSTATE[" . $sqlstate . "]: " . $message . "\n");
    }

    // F-CORE-04/F-CORE-05: the boundary between "an attribute number this PDO surface
    // knows about" and "a number that is not a PDO attribute at all". php-src's
    // setAttribute falls through to `pdo_raise_impl_error(dbh, NULL, "IM001", "driver
    // does not support setting attributes")`, and getAttribute to IM001 "driver does not
    // support that attribute", once neither the generic switch nor the driver's own hook
    // claims the attribute. This prelude used to store-and-return-true for ANY integer
    // and read back null for anything unlisted, so `setAttribute(9999, 'x')` reported
    // success and `getAttribute(9999)` was indistinguishable from a legitimately-null
    // attribute.
    //
    // The boundary is drawn on the attribute NUMBER, deliberately, and deliberately
    // GENEROUSLY — the goal is to name nonsense, not to police attributes real code
    // legitimately round-trips:
    //  - 0..21 is the whole CONTIGUOUS generic PDO_ATTR_* space this class declares
    //    (ATTR_AUTOCOMMIT=0 … ATTR_DEFAULT_STR_PARAM=21). Several of these are acted on
    //    by no driver here but ARE stored and echoed back through $this->attributes
    //    (ATTR_STATEMENT_CLASS, ATTR_CURSOR, ATTR_EMULATE_PREPARES…), and callers depend
    //    on that — including this prelude's own prepare(), which snapshots attribute 20
    //    via getAttribute() — so the whole range stays accepted and stored.
    //  - 1000..1015 is the driver-specific range: PDO_ATTR_DRIVER_SPECIFIC (1000) up to
    //    the highest constant any of the three driver subclasses declares
    //    (Pdo\Mysql::ATTR_LOCAL_INFILE_DIRECTORY = 1015). The drivers deliberately OVERLAP
    //    in this range (1002 is both Pdo\Sqlite::ATTR_EXTENDED_RESULT_CODES and
    //    Pdo\Mysql::ATTR_INIT_COMMAND), and php-src likewise hands the entire range to the
    //    driver hook rather than validating it per-driver at the PDO layer, so it is
    //    accepted wholesale rather than narrowed against the live connection's driver.
    // Everything else — a negative number, 22..999, anything above 1015 — is what IM001
    // now names.
    private function isKnownAttribute(int $attribute): bool {
        if ($attribute >= 0 && $attribute <= 21) {
            return true;
        }
        return $attribute >= 1000 && $attribute <= 1015;
    }

    // P1-h: ATTR_ERRMODE (3) only accepts PDO::ERRMODE_SILENT/WARNING/EXCEPTION
    // (0/1/2); anything else throws a ValueError and leaves the current mode
    // untouched — shared by setAttribute() and the constructor's $options loop.
    private function checkErrMode(int $mode): void {
        if ($mode != 0 && $mode != 1 && $mode != 2) {
            throw new ValueError("Error mode must be one of the PDO::ERRMODE_* constants");
        }
    }

    // P1-h/P3: ATTR_DEFAULT_FETCH_MODE (19) rejects only PDO::FETCH_USE_DEFAULT
    // (0, i.e. "no mode") — shared by setAttribute() and the constructor's
    // $options loop. Divergence check against php-src's pdo_dbh.c (verified):
    // real PHP's FETCH_CLASS/FETCH_INTO rejection ("PDO::FETCH_INTO and
    // PDO::FETCH_CLASS cannot be set as the default fetch mode") ONLY fires
    // when the given value is an ARRAY whose element [0] is one of those modes
    // (the `setAttribute(ATTR_DEFAULT_FETCH_MODE, [PDO::FETCH_CLASS, 'Foo'])`
    // idiom); a BARE int 8/9 is accepted and stored like any other mode. Since
    // elephc's setAttribute() takes a plain `mixed $value` and this prelude
    // only ever narrows it with `(int) $value`, the array-form never reaches
    // here at all, so there is no elephc analogue of that rejection to mirror.
    private function checkDefaultFetchMode(int $mode): void {
        if ($mode == 0) {
            throw new ValueError("Fetch mode must be a bitmask of PDO::FETCH_* constants");
        }
    }

    // P2-e: ATTR_CASE (8) only accepts PDO::CASE_NATURAL/CASE_UPPER/CASE_LOWER
    // (0/1/2); anything else throws a ValueError with the exact message php-src's
    // pdo_dbh.c uses (verified against php-src) — shared by setAttribute() and the
    // constructor's $options loop. Divergence: PDO::ATTR_ORACLE_NULLS (11) has NO
    // equivalent check in real PHP either (pdo_dbh.c carries a
    // `/* TODO Check for valid value */` comment and stores whatever integer is
    // given), so there is no analogous helper for it here; PDOStatement's fetch
    // path only pattern-matches NULL_EMPTY_STRING(1)/NULL_TO_STRING(2) and treats
    // every other stored value as a no-op natural mode, mirroring that unchecked
    // acceptance exactly.
    private function checkAttrCase(int $mode): void {
        if ($mode != 0 && $mode != 1 && $mode != 2) {
            throw new ValueError("Case folding mode must be one of the PDO::CASE_* constants");
        }
    }

    // F-CORE-03: php-src names the offending value with zend_zval_value_name() in
    // the TypeError the two helpers below raise; mirror the spellings it produces
    // for every shape a PHP-level attribute value can actually reach here.
    private function attrValueTypeName(mixed $value): string {
        if (is_int($value)) {
            return "int";
        }
        if (is_bool($value)) {
            return "bool";
        }
        if (is_float($value)) {
            return "float";
        }
        if (is_string($value)) {
            return "string";
        }
        if (is_array($value)) {
            return "array";
        }
        if (is_null($value)) {
            return "null";
        }
        return "object";
    }

    // F-CORE-03 (SECURITY-adjacent): php-src checks the SHAPE of an attribute
    // value BEFORE any per-attribute range check — pdo_get_long_param() accepts
    // only IS_LONG, IS_TRUE/IS_FALSE, or a string that is_numeric_str_function()
    // reports as IS_LONG, and raises a TypeError otherwise. This prelude used to
    // cast blindly with `(int) $value`, and `(int) "banana"` is 0 — which is
    // PDO::ERRMODE_SILENT, a value checkErrMode() happily accepts — so
    // `setAttribute(PDO::ATTR_ERRMODE, "banana")` silently switched the connection
    // to SILENT and swallowed every subsequent error. Shared by setAttribute() and
    // the constructor's $options loop, which had the identical blind-cast problem.
    private function attrIntValue(mixed $value): int {
        if (is_int($value) || is_bool($value)) {
            return (int) $value;
        }
        if (is_string($value)) {
            $_sval = (string) $value;
            // php-src takes a string only when it parses as IS_LONG, so an
            // INTEGER-shaped numeric string passes while a float-shaped one
            // ("1.5", "1e3" — both IS_DOUBLE) falls through to the TypeError;
            // is_numeric() alone would wrongly accept those, hence the explicit
            // fractional/exponent rejection.
            if (is_numeric($_sval) && strpos($_sval, ".") === false && strpos($_sval, "e") === false && strpos($_sval, "E") === false) {
                return (int) $_sval;
            }
        }
        throw new TypeError("Attribute value must be of type int for selected attribute, " . $this->attrValueTypeName($value) . " given");
    }

    // F-CORE-03: the bool-typed counterpart, mirroring pdo_get_bool_param() —
    // only IS_TRUE/IS_FALSE/IS_LONG are accepted there (its `case IS_STRING:`
    // deliberately falls through to the TypeError, so a string is NOT a valid
    // bool attribute value even when it looks like one).
    private function attrBoolValue(mixed $value): bool {
        if (is_bool($value) || is_int($value)) {
            return (bool) $value;
        }
        throw new TypeError("Attribute value must be of type bool for selected attribute, " . $this->attrValueTypeName($value) . " given");
    }

    public function setAttribute(int $attribute, $value): bool {
        if ($attribute == 3) {
            // F-CORE-03: the shape check runs BEFORE the range check, exactly as
            // php-src's pdo_get_long_param() does — see attrIntValue() for why a
            // blind cast here was actively dangerous for ATTR_ERRMODE.
            $_attrErrMode = $this->attrIntValue($value);
            $this->checkErrMode($_attrErrMode);
            $this->errMode = $_attrErrMode;
        } elseif ($attribute == 12) {
            $this->persistent = $this->attrBoolValue($value);
        } elseif ($attribute == 2) {
            // ATTR_TIMEOUT: SQLite maps it to a busy-timeout; PHP's unit is
            // seconds, SQLite's is milliseconds. Other drivers accept it as a
            // no-op (see the bridge).
            elephc_pdo_set_busy_timeout($this->conn, $this->attrIntValue($value) * 1000);
        } elseif ($attribute == 19) {
            $_attrFetchMode = $this->attrIntValue($value);
            $this->checkDefaultFetchMode($_attrFetchMode);
            $this->defaultFetchMode = $_attrFetchMode;
        } elseif ($attribute == 17) {
            $this->stringifyFetches = $this->attrBoolValue($value);
        } elseif ($attribute == 8) {
            $_attrCase = $this->attrIntValue($value);
            $this->checkAttrCase($_attrCase);
            $this->attrCase = $_attrCase;
        } elseif ($attribute == 11) {
            $this->oracleNulls = $this->attrIntValue($value);
        } elseif ($attribute == 1002 && elephc_pdo_driver_name($this->conn) === "sqlite") {
            // F-SQLT-02: Pdo\Sqlite::ATTR_EXTENDED_RESULT_CODES. php-src's
            // pdo_sqlite_set_attribute calls sqlite3_extended_result_codes(), which
            // widens the driver-specific code in errorInfo[1] from the coarse primary
            // code (SQLITE_CONSTRAINT, 19) to the extended one that says WHICH
            // constraint failed (SQLITE_CONSTRAINT_UNIQUE, 2067) — the difference
            // between "a constraint broke" and an actionable error.
            //
            // The driver guard is required, not defensive noise: 1002 is a colliding
            // number. It is Pdo\Mysql::ATTR_INIT_COMMAND (a STRING, consumed at
            // connect time by the constructor's $options loop) on a mysql: connection,
            // so an unguarded branch would push that string through attrBoolValue()
            // and raise a spurious TypeError. Each driver owns its own 1000+ range;
            // this attribute only means "extended result codes" for sqlite:.
            elephc_pdo_set_extended_result_codes($this->conn, $this->attrBoolValue($value) ? 1 : 0);
        } elseif (!$this->isKnownAttribute($attribute)) {
            // F-CORE-04 (CORRECTED — the finalization spec was WRONG about this, and an
            // earlier pass implemented the spec's version): an UNKNOWN attribute number
            // makes real PHP's setAttribute() return **false SILENTLY**. It raises
            // nothing — no exception, no error state — not even under
            // ERRMODE_EXCEPTION. VERIFIED against a real PHP 8.5.6 CLI:
            // `$pdo->setAttribute(9999, 1)` on an ERRMODE_EXCEPTION handle returns
            // bool(false) and `$pdo->errorCode()` still reads "00000".
            //
            // WHY, in php-src's own terms: pdo_dbh_attribute_set() only reaches
            // `pdo_raise_impl_error(…, "IM001", "driver does not support setting
            // attributes")` on the `!dbh->methods->set_attribute` arm — a driver with NO
            // set_attribute hook AT ALL. All three drivers this bridge implements
            // (pdo_sqlite, pdo_mysql, pdo_pgsql) HAVE one, and each simply `return 0`s
            // for an attribute it does not recognize WITHOUT setting an error, so the
            // PDO_HANDLE_DBH_ERR() that follows finds SQLSTATE "00000" and raises
            // nothing. The IM001 arm is therefore unreachable for every driver here.
            //
            // getAttribute() is GENUINELY ASYMMETRIC and its IM001 (further down) stays:
            // pdo_sqlite's get_attribute hook returning 0 lands on an EXPLICIT
            // pdo_raise_impl_error, so `getAttribute(9999)` really does throw on a real
            // CLI. The asymmetry looks like a bug in php-src; it is nonetheless the
            // behavior, and mirroring it is the whole point of this surface.
            //
            // What DOES survive from the original finding: NOTHING is stored. The old
            // code's store-and-return-TRUE was wrong under any reading — a rejected
            // attribute must not read back out of getAttribute() — so the reject
            // boundary (isKnownAttribute(), see its own comment) still governs; only the
            // loudness of the rejection changes.
            return false;
        }
        $this->attributes[$attribute] = $value;
        return true;
    }

    public function getAttribute(int $attribute): mixed {
        if ($attribute == 3) {
            return $this->errMode;
        }
        if ($attribute == 12) {
            return $this->persistent;
        }
        if ($attribute == 16) {
            return elephc_pdo_driver_name($this->conn);
        }
        if ($attribute == 19) {
            return $this->defaultFetchMode;
        }
        if ($attribute == 17) {
            return $this->stringifyFetches;
        }
        if ($attribute == 8) {
            return $this->attrCase;
        }
        if ($attribute == 11) {
            return $this->oracleNulls;
        }
        if ($attribute == 4) {
            return elephc_pdo_server_version($this->conn);
        }
        // P2-13: ATTR_CLIENT_VERSION (5). The bridge has no distinct client-library
        // version accessor — it links each driver crate straight into the binary
        // rather than dynamically loading a client lib — so reuse the server-version
        // accessor as the cheapest real value. For sqlite this is exact PHP parity
        // (pdo_sqlite is embedded and reports the SAME string for both attributes,
        // verified against a real PHP CLI); for pgsql/mysql it stands in for a
        // driver-native client version this bridge does not separately expose.
        if ($attribute == 5) {
            return elephc_pdo_server_version($this->conn);
        }
        // P2-l: ATTR_SERVER_INFO (6) is intentionally left unwired. php-src only
        // answers this for MySQL, from mysqlnd's own live `mysql_stat()` admin
        // string (uptime/threads/queries/etc., via the COM_STATISTICS wire
        // command); pdo_pgsql/pdo_sqlite have no equivalent and fall through to
        // NULL. Neither the `mysql` crate nor `mysql_common` this bridge links
        // exposes a COM_STATISTICS/mysql_stat() accessor — that wire command
        // exists in mysql_common's protocol constants but is never sent by any
        // public API the client crate offers — so producing one would mean
        // hand-rolling that packet, which is more than a "cheap" accessor. This
        // falls through to the generic $this->attributes lookup below (null
        // unless a caller has explicitly setAttribute(6, ...)'d something),
        // matching the NULL php-src itself returns for pgsql/sqlite.
        //
        // P2-13/P3: ATTR_CONNECTION_STATUS (7). Real drivers report a live libpq
        // PQstatus()/mysqlnd mysql_stat() socket status string; the bridge has no
        // such accessor. getAttribute() only runs on a PDO object whose connection
        // is still open (a closed connection's methods are unreachable through
        // normal use), so a static string is accurate for elephc's model. The
        // literal matches php-src exactly: "Connection OK; waiting to send." is
        // the only string a real, freshly-opened PostgreSQL/MySQL connection
        // observably reports (libpq's PQstatus()==CONNECTION_OK / mysqlnd's own
        // "waiting to send" state) for as long as nothing else is in flight.
        if ($attribute == 7) {
            return "Connection OK; waiting to send.";
        }
        // F-SQLT-02 (DECISION: php-src parity, not echo-back).
        // Pdo\Sqlite::ATTR_EXTENDED_RESULT_CODES (1002) is WRITE-ONLY in real PHP:
        // pdo_sqlite_set_attribute handles it, but pdo_sqlite_get_attribute has NO
        // case for it, so PDO falls through and getAttribute() yields NULL — it does
        // not echo back what you set, and there is no sqlite3 C API to read the flag
        // back either. The generic $this->attributes lookup below WOULD echo it back
        // (setAttribute stores every attribute unconditionally), which is exactly the
        // divergence this early return exists to prevent.
        //
        // Scoped to sqlite: because 1002 is also Pdo\Mysql::ATTR_INIT_COMMAND (see
        // setAttribute) — a different attribute, on a different driver, that this
        // prelude does read back out of $this->attributes.
        if ($attribute == 1002 && elephc_pdo_driver_name($this->conn) === "sqlite") {
            return null;
        }
        if (isset($this->attributes[$attribute])) {
            return $this->attributes[$attribute];
        }
        // F-CORE-05: php-src's getAttribute fall-through — IM001 "driver does not support
        // that attribute" once the generic switch AND the driver hook have both declined
        // (pdo_dbh.c's `case 0:` arm), returning FALSE (php-src's literal `RETURN_FALSE`,
        // not NULL). errMode-aware like every other synthetic failure: ERRMODE_SILENT and
        // ERRMODE_WARNING still get `false` back rather than a throw. Unlike setAttribute's
        // IM001 (see the divergence note there), THIS one is exactly what real PHP does:
        // `(new PDO("sqlite::memory:"))->getAttribute(9999)` on a real 8.5.6 CLI throws
        // `SQLSTATE[IM001] … driver does not support that attribute`.
        //
        // A KNOWN attribute number with nothing stored keeps returning null, unchanged —
        // that is this surface's long-standing "the attribute exists, nobody set it"
        // answer (ATTR_SERVER_INFO's documented null above depends on reaching exactly
        // here), and the finding is about numbers that are not attributes at all.
        if (!$this->isKnownAttribute($attribute)) {
            $this->failCode("IM001", "driver does not support that attribute");
            return false;
        }
        return null;
    }

    public function exec(string $statement): int|bool {
        // F-CORE-21/P2-f: real PHP validates this before any driver call at all —
        // php-src's PHP_METHOD(PDO, exec) raises the ValueError from its own
        // argument check, exactly like the prepare() guard just below (which this
        // method was inconsistently missing, so `exec("")` reached the bridge).
        if ($statement === "") {
            throw new ValueError("PDO::exec(): Argument #1 (\$statement) must not be empty");
        }
        $_affected = elephc_pdo_exec($this->conn, $statement);
        if ($_affected < 0) {
            $this->fail(elephc_pdo_errmsg($this->conn));
            return false;
        }
        return $_affected;
    }

    public function prepare(string $query, array $options = []): PDOStatement|bool {
        // P2-f: real PHP validates this before any driver call at all.
        if ($query === "") {
            throw new ValueError("PDO::prepare(): Argument #1 (\$query) must not be empty");
        }
        $_handle = elephc_pdo_prepare($this->conn, $query);
        if ($_handle < 0) {
            $this->fail(elephc_pdo_errmsg($this->conn));
            return false;
        }
        // Inherit the connection's default fetch mode (ATTR_DEFAULT_FETCH_MODE) so
        // a statement fetched with no explicit mode uses the dbh default.
        $_stmt = new PDOStatement($_handle, $this->conn, $this->errMode, $query);
        // P1-j: root the owning PDO (and its bridge connection) on the new
        // statement so it survives past the scope of any local variable
        // holding this PDO — see PDOStatement::$owner / setOwner().
        $_stmt->setOwner($this);
        // P3: propagates the raw stored default, bypassing setFetchMode()'s own
        // argument validation — see setDefaultFetchMode()'s comment for why a
        // prepare()-time call must not run through that validation.
        $_stmt->setDefaultFetchMode($this->defaultFetchMode);
        // P1-11: inherit ATTR_STRINGIFY_FETCHES the same way (a prepare()-time
        // snapshot, not a live read — see the property comment on
        // $stringifyFetches above).
        $_stmt->setStringifyFetches($this->stringifyFetches);
        // P1-i: snapshot ATTR_EMULATE_PREPARES the same way, so
        // PDOStatement::getAttribute(ATTR_EMULATE_PREPARES) answers from the
        // owning connection's stored value (or false when never set) instead of
        // raising IM001 like every other unsupported statement attribute.
        $_stmt->setEmulatePrepares((bool) $this->getAttribute(20));
        // P2-e: snapshot ATTR_CASE / ATTR_ORACLE_NULLS the same way (see the
        // property comments on $attrCase/$oracleNulls above).
        $_stmt->setAttrCase($this->attrCase);
        $_stmt->setOracleNulls($this->oracleNulls);
        // $options (PDO::ATTR_CURSOR, driver-specific prepare hints, ...) is accepted
        // for signature compatibility with callers like Doctrine's driver layer, but
        // is intentionally NOT iterated here: none of the supported prepare options
        // has a behavioral effect, and a `foreach ($options ...)` inside this ordinary
        // (non-top-level) function frame trips a pre-existing EIR miscompile — the
        // foreach-iterator local is not re-initialized between differently-shaped
        // invocations of the same function, so a later `prepare($sql, [k=>v])` after an
        // earlier `prepare($sql)` corrupts the heap (the "C2a" wild-write class tracked
        // for #511). Accept-and-ignore is fully PHP-compatible for every option elephc
        // does not act on, and sidesteps that miscompile entirely.
        $_ignoredOptions = $options;
        return $_stmt;
    }

    public function query(string $query, ?int $fetchMode = null, mixed $arg1 = null, mixed $arg2 = null): PDOStatement|bool {
        // F-CORE-22: php-src's PHP_METHOD(PDO, query) carries its OWN empty-statement
        // check, so this must not be left to the prepare() call below — an empty query
        // did throw, but under the wrong method name ("PDO::prepare(): ..."). php-src's
        // own message names the argument `$statement` here (the C-level parameter its
        // check validates) even though this prelude's parameter is `$query`; keep
        // php-src's text verbatim so a caller matching on the message sees real PHP's.
        if ($query === "") {
            throw new ValueError("PDO::query(): Argument #1 (\$statement) must not be empty");
        }
        $_statement = $this->prepare($query);
        if ($_statement === false) {
            return false;
        }
        if ($_statement->execute() === false) {
            return false;
        }
        if ($fetchMode !== null) {
            // Bounded fallback for PHP's `query(string, ?int, mixed ...$fetchModeArgs)`:
            // elephc's checker cannot yet type-check a heterogeneous variadic tail
            // declared on a class METHOD (a leading non-variadic parameter makes the
            // checker mis-derive both the minimum arity and the variadic element
            // type), so this accepts up to two extra args instead. $arg1 covers
            // FETCH_COLUMN's column index and FETCH_CLASS/FETCH_INTO's target, same
            // as setFetchMode()'s existing second parameter. $arg2 (e.g. FETCH_CLASS
            // constructor args) is accepted but not forwarded, like fetchObject()'s
            // documented $constructorArgs divergence.
            $_unusedArg2 = $arg2;
            // Explicit (int) cast: the checker does not narrow a `?int` parameter
            // to `int` from the `!== null` guard above when it flows into another
            // method call's argument, so an uncast $fetchMode fails to type-check
            // against setFetchMode()'s `int $mode` parameter.
            $_statement->setFetchMode((int) $fetchMode, $arg1);
        }
        return $_statement;
    }

    public function lastInsertId(?string $name = null): string|bool {
        // The name is a sequence for PostgreSQL (`currval($name)`); SQLite and
        // MySQL ignore it and return the last rowid / auto-increment id. The text
        // bridge is used so oversized PostgreSQL sequence values (which need not
        // fit in an i64) round-trip without truncation.
        //
        // F-CORE-18: php-src's signature is `string|false`. SQLite and MySQL
        // return "0" (never "") when there was no insert, and PostgreSQL's
        // `lastval()` errors when no sequence has been used in the session
        // (SQLSTATE 55000); the bridge reports every such failure — and an
        // unknown handle — as "". An empty result is therefore the failure
        // sentinel: surface the connection's real error when the driver set one
        // (error-mode-aware, via failCode()), else a generic IM001, and return
        // false rather than silently handing back "".
        $_id = elephc_pdo_last_insert_id_text($this->conn, $name ?? "");
        if ($_id !== "") {
            return $_id;
        }
        $_sqlstate = elephc_pdo_sqlstate($this->conn);
        if ($_sqlstate !== "00000") {
            $this->failCode($_sqlstate, elephc_pdo_errmsg($this->conn));
        } else {
            $this->failCode("IM001", "driver does not support lastInsertId()");
        }
        return false;
    }

    public function beginTransaction(): bool {
        // PHP forbids nesting: starting a transaction while one is active is a
        // logic error and throws regardless of the error mode. P1-g: consult the
        // driver's LIVE transaction state where one exists, so a transaction
        // started by a raw exec("BEGIN") — bypassing this method — is caught
        // too, matching php-src asking the driver instead of trusting a
        // PHP-side flag. -1 means the driver has no live read (pgsql/mysql);
        // stay defensive there and only raise the guard when $inTxn itself says
        // a transaction is active, rather than treating "unknown" as "active".
        $_live = elephc_pdo_in_transaction($this->conn);
        $_alreadyActive = $_live === 1 || ($_live === -1 && $this->inTxn);
        if ($_alreadyActive) {
            throw new PDOException("There is already an active transaction");
        }
        if (elephc_pdo_begin($this->conn) != 1) {
            $this->fail(elephc_pdo_errmsg($this->conn));
            return false;
        }
        $this->inTxn = true;
        return true;
    }

    public function commit(): bool {
        // Committing without an active transaction is a logic error in PHP.
        if (!$this->inTxn) {
            throw new PDOException("There is no active transaction");
        }
        if (elephc_pdo_commit($this->conn) != 1) {
            $this->fail(elephc_pdo_errmsg($this->conn));
            return false;
        }
        $this->inTxn = false;
        return true;
    }

    public function rollBack(): bool {
        // Rolling back without an active transaction is a logic error in PHP.
        if (!$this->inTxn) {
            throw new PDOException("There is no active transaction");
        }
        if (elephc_pdo_rollback($this->conn) != 1) {
            $this->fail(elephc_pdo_errmsg($this->conn));
            return false;
        }
        $this->inTxn = false;
        return true;
    }

    public function inTransaction(): bool {
        // P1-g: prefer the driver's LIVE transaction state (matching php-src,
        // which asks the driver rather than trusting client-side bookkeeping) —
        // this is what makes a transaction started via a raw exec("BEGIN")
        // visible here. -1 means the driver has no live read (pgsql/mysql, see
        // the extern's docblock); fall back to the $inTxn flag maintained by
        // beginTransaction()/commit()/rollBack() in that case.
        $_live = elephc_pdo_in_transaction($this->conn);
        if ($_live === 0 || $_live === 1) {
            return $_live === 1;
        }
        return $this->inTxn;
    }

    public static function getAvailableDrivers(): array {
        // The drivers this bridge can dispatch to from a DSN prefix.
        return ["mysql", "pgsql", "sqlite"];
    }

    public static function connect(string $dsn, ?string $username = null, ?string $password = null, ?array $options = null): PDO {
        // PHP 8.4 static factory: dispatch on the DSN driver prefix and return an
        // instance of the matching driver-specific subclass. Each subclass inherits
        // the whole \PDO surface, so the returned object opens the connection and
        // behaves exactly like `new PDO($dsn, ...)`; only its concrete class differs,
        // so `PDO::connect("sqlite:...") instanceof \Pdo\Sqlite` is true. Declared to
        // return the base \PDO because the subclasses ARE \PDO and elephc has no
        // `static` return type; the runtime object is the exact subclass. An
        // unrecognized prefix throws, matching PHP's "could not find driver".
        //
        // F-CORE-01 (divergence that SURVIVES this wave): php-src rejects a
        // subclass-qualified mismatched call — `Pdo\Sqlite::connect("mysql:…")` throws
        // "…cannot be used for connecting to the \"mysql\" driver…", with "connect()"
        // in place of "__construct()". The `new Pdo\Sqlite("mysql:…")` spelling IS now
        // rejected (see \PDO::checkDriverSubclassDsn(), run from each subclass
        // constructor), but this STATIC form cannot be: elephc has no late static
        // binding — `static::` lowers to the DEFINING class (src/ir_lower/expr/mod.rs:9654)
        // — so an inherited static method cannot observe which subclass it was called
        // through, and this factory therefore still dispatches on the DSN prefix alone.
        if (str_starts_with($dsn, "sqlite:")) {
            return new \Pdo\Sqlite($dsn, $username, $password, $options);
        }
        if (str_starts_with($dsn, "mysql:")) {
            return new \Pdo\Mysql($dsn, $username, $password, $options);
        }
        if (str_starts_with($dsn, "pgsql:")) {
            return new \Pdo\Pgsql($dsn, $username, $password, $options);
        }
        throw new PDOException("could not find driver");
    }

    protected function connectionId(): int {
        // The raw bridge connection handle, exposed to driver subclasses (e.g.
        // Pdo\Pgsql::getPid, Pdo\Mysql::getWarningCount) so they can reach the
        // connection without widening the private $conn property. Called through
        // normal inherited method dispatch, so it reads $conn in the base class's
        // own scope.
        return $this->conn;
    }

    protected function blobStream(int $length): mixed {
        // Turns a whole-BLOB / whole-large-object read into the read-whole resource
        // that Pdo\Sqlite::openBlob() / Pdo\Pgsql::lobOpen() return. The caller has
        // already populated the shared bridge buffer (elephc_pdo_blob_read /
        // elephc_pdo_lob_get) and passes its byte length here; a negative length
        // signals a bridge error and yields false. The buffer is copied out in ONE
        // call and wrapped in a rewound in-memory read/write stream. This is a
        // read-whole snapshot: writing back to the stream does not update the stored
        // BLOB / large object.
        //
        // F-QUAL-01: this used to drain the buffer one byte at a time through
        // elephc_pdo_blob_byte()+chr() — one FFI call (and one string concatenation)
        // per byte, which for a BLOB is exactly the value class where that is most
        // expensive. blob_data_ptr + ptr_read_string copies an EXACT byte count with
        // no NUL-termination semantics, so embedded NUL bytes still survive into the
        // PHP string, which was the byte loop's whole reason for existing. As in
        // columnValue(), the zero-length guard is required rather than merely tidy:
        // the bridge hands back a NULL pointer for an empty buffer and
        // ptr_read_string fatals on NULL before it ever inspects the length.
        if ($length < 0) {
            return false;
        }
        $_data = "";
        if ($length > 0) {
            $_data = \ptr_read_string(\elephc_pdo_blob_data_ptr(), $length);
        }
        $_stream = \fopen("php://memory", "r+");
        \fwrite($_stream, $_data);
        \rewind($_stream);
        return $_stream;
    }

    public function errorCode(): ?string {
        // The 5-character SQLSTATE for the connection's last operation ("00000"
        // on success). Divergence from PHP: this returns "00000" rather than null
        // before the first operation, because the bridge reports a fresh handle's
        // state as success.
        return elephc_pdo_sqlstate($this->conn);
    }

    public function errorInfo(): array {
        // PHP's errorInfo() is [SQLSTATE, driver-specific code, message], with
        // ["00000", null, null] on success. Every driver surfaces a real SQLSTATE:
        // SQLite via a php-src-matching table, MySQL from the ERR packet's
        // #-marked field, PostgreSQL from the ErrorResponse 'C' field.
        $_sqlstate = elephc_pdo_sqlstate($this->conn);
        if ($_sqlstate === "00000") {
            return ["00000", null, null];
        }
        return [$_sqlstate, elephc_pdo_errcode($this->conn), elephc_pdo_errmsg($this->conn)];
    }

    public function quote(string $string, int $type = 2): string {
        // Driver-aware string-literal quoting. PDO::PARAM_LOB (3, P1-e) selects a
        // driver-native binary literal instead of the plain string-escaping path;
        // every other $type value is accepted for PHP signature compatibility but
        // otherwise ignored, matching php-src's own quoters (which only ever
        // special-case PARAM_LOB). Prepared statements remain the recommended
        // path; quote() is only safe when it matches the target driver's literal
        // syntax, so it branches on the driver name.
        $_driver = elephc_pdo_driver_name($this->conn);
        if ($_driver === "mysql") {
            if (elephc_pdo_no_backslash_escapes($this->conn) != 0) {
                // P1-f (SECURITY): under the MySQL NO_BACKSLASH_ESCAPES sql_mode,
                // backslash is a literal character inside a string literal, so
                // backslash-escaping is actively unsafe there — an escaped quote
                // (\') does not escape at all and lets a crafted string break out
                // of the literal. mysqlnd itself switches to quote-doubling-only
                // in that mode; mirror that via the bridge's live sql_mode read.
                $_s = str_replace("'", "''", $string);
            } else {
                // MySQL: ''-doubling alone is injectable with a trailing-backslash
                // payload, so backslash-escape. Escape the backslash first, then the
                // quotes and the control bytes MySQL recognizes in string literals.
                $_s = str_replace("\\", "\\\\", $string);
                $_s = str_replace("'", "\\'", $_s);
                $_s = str_replace("\"", "\\\"", $_s);
                $_s = str_replace(chr(0), "\\0", $_s);
                $_s = str_replace(chr(10), "\\n", $_s);
                $_s = str_replace(chr(13), "\\r", $_s);
                $_s = str_replace(chr(26), "\\Z", $_s);
            }
            $_quoted = "'" . $_s . "'";
            if ($type == 3) {
                // PDO::PARAM_LOB (P1-e): mirrors php-src's mysql_handle_quoter,
                // which prefixes the escaped literal with the `_binary` charset
                // introducer so the byte string is treated as opaque binary data
                // rather than reinterpreted under the connection's charset.
                return "_binary" . $_quoted;
            }
            return $_quoted;
        }
        if ($_driver === "pgsql") {
            if ($type == 3) {
                // PDO::PARAM_LOB (P1-e): a bytea hex-format literal
                // ('\xDEADBEEF...') is always valid regardless of the server's
                // bytea_output setting and is binary-safe (an embedded NUL byte
                // survives), unlike the standard-conforming-strings-sensitive
                // escape path below — mirrors php-src's PQescapeByteaConn call.
                return "'\\x" . bin2hex($string) . "'";
            }
            // PostgreSQL: double single quotes; if a backslash is present, use the
            // E'...' escape-string form so backslashes are taken literally
            // regardless of standard_conforming_strings.
            $_doubled = str_replace("'", "''", $string);
            if (strpos($string, "\\") !== false) {
                return "E'" . str_replace("\\", "\\\\", $_doubled) . "'";
            }
            return "'" . $_doubled . "'";
        }
        // SQLite (and the default): standard SQL ''-doubling is correct, and
        // $type is ignored here too — matching php-src's own sqlite quoter,
        // which never consults the type argument either.
        return "'" . str_replace("'", "''", $string) . "'";
    }

    public function __destruct() {
        // Release the bridge connection when the PDO object is collected. An open
        // transaction is rolled back first (matching PHP and keeping a persistent
        // handle clean when it returns to the pool). The bridge finalizes the
        // connection's remaining statements before closing, and treats an
        // already-closed handle as a no-op, so the order relative to any surviving
        // PDOStatement destructors does not matter.
        if ($this->inTxn) {
            elephc_pdo_rollback($this->conn);
            $this->inTxn = false;
        }
        elephc_pdo_close($this->conn);
    }

    // P2-17: PHP marks PDO uncloneable — `clone $pdo` throws an `Error` before any
    // property is copied, rather than producing a second Zend object that shares the
    // one bridge connection handle. Without this guard elephc's default shallow clone
    // would hand back a second owner of `$this->conn`; whichever copy is destructed
    // first closes the connection out from under the survivor. `get_class($this)`
    // reports the runtime (possibly driver-subclass) class name, matching PHP's exact
    // message on e.g. `clone (new \Pdo\Sqlite(...))`.
    public function __clone(): void {
        throw new Error("Trying to clone an uncloneable object of class " . get_class($this));
    }

    // F-CORE-15 (SECURITY-adjacent): php-src marks `class PDO` — and PDOStatement —
    // `/** @not-serializable */` in ext/pdo/pdo.stub.php, which installs
    // zend_class_serialize_deny, so `serialize($pdo)` throws
    // `Exception: Serialization of 'PDO' is not allowed`. elephc has no per-class engine
    // flag for that, and its serialize() simply WALKED THE PROPERTIES: it emitted a blob
    // containing this object's private `$conn` — the raw integer bridge handle — and
    // unserialize() handed back a zombie PDO whose handle indexes nothing (every bridge
    // call then answers with an unknown-handle sentinel: driver_name "", errcode 0…).
    // Silent misbehavior where php-src is loud, and a serialized blob that leaks internal
    // handle numbering into whatever store it lands in.
    //
    // elephc's serialize() DOES honor the magic hooks, so this is enforceable from the
    // prelude: __rt_serialize_object consults the per-class `_class_serialize_ptrs` table
    // FIRST and falls back to `_class_sleep_ptrs`
    // (src/codegen_support/runtime/system/serialize.rs:559-636; both tables are emitted
    // per class_id, resolving through the implementing class so subclasses inherit the
    // entry — src/codegen_support/runtime/data/user.rs:288-306). BOTH are declared here:
    // __serialize() is the one that actually fires today, __sleep() is the fallback the
    // runtime reaches when a class has no __serialize(), and declaring both means no
    // ordering change in that runtime can ever quietly re-open the property-walk path.
    // The throw unwinds out of the runtime's serialize frame through the ordinary
    // longjmp-to-handler path, like any exception raised inside a magic method.
    //
    // get_class($this), not a literal "PDO": php-src's deny handler names the OBJECT's
    // class, so `serialize(new \Pdo\Sqlite(...))` reports
    // `Serialization of 'Pdo\Sqlite' is not allowed` — and the subclasses inherit these
    // two methods, so they get that message for free. The thrown class is a plain
    // `Exception` (not PDOException): zend_class_serialize_deny passes a NULL class entry
    // to zend_throw_exception_ex, which is the base Exception.
    public function __serialize(): array {
        throw new Exception("Serialization of '" . get_class($this) . "' is not allowed");
    }

    public function __sleep(): array {
        throw new Exception("Serialization of '" . get_class($this) . "' is not allowed");
    }
}

// F-STMT-11 (INVESTIGATED, DELIBERATELY NOT CHANGED) — php-src declares
// `class PDOStatement implements IteratorAggregate`, with an INTERNAL, userland-invisible
// iterator object behind its get_iterator handler (pdo_stmt_iter_*). elephc implements
// `Iterator` directly. foreach BEHAVIOR is identical either way (both walk the forward-only
// cursor in the statement's current fetch mode with sequential integer keys); what differs
// is reflection:
//     $stmt instanceof \Iterator            elephc: true    php: false
//     $stmt instanceof \IteratorAggregate    elephc: false   php: true
//     method_exists($stmt, 'rewind'|'valid'|'current'|'key'|'next')
//                                            elephc: true    php: false
// (`$stmt instanceof \Traversable` is true in BOTH, which is what every framework actually
// tests, and `foreach`/`getIterator()` work in both.)
//
// The switch was investigated and is NOT blocked by the compiler: elephc supports
// IteratorAggregate end-to-end, including a `getIterator(): Iterator` returning a separate
// concrete iterator, both for a statically-typed source and for a dynamic/union-typed one
// (src/codegen/lower_inst/iterators.rs: object_iterator_source /
// resolve_dynamic_object_iterator_source). It is not taken because the trade is bad:
//   * elephc cannot express php's INTERNAL iterator. The aggregate form needs a real,
//     userland-visible class (`class PDOStatementIterator implements Iterator`) holding the
//     statement — so it would remove five public methods php does not have by ADDING a
//     public class php does not have. The reflection divergence moves; it does not vanish.
//   * every `foreach ($stmt as ...)` in existing code — including all statements coming out
//     of `query()`/`prepare()`, whose declared return type is the UNION `PDOStatement|bool`
//     and therefore lowers through the DYNAMIC iterator path — would switch to a different
//     codegen path (getIterator() dispatch + source replacement) for a purely cosmetic
//     reflection gain. foreach over a PDOStatement is the single most-used PDO idiom here.
// A regression in foreach is strictly worse than a divergence in `instanceof`, so the
// divergence above is documented and kept. Re-open only alongside a compiler feature for
// non-public/internal classes.
class PDOStatement implements Iterator {
    private int $stmt;
    private int $conn;
    private int $errMode;
    private int $fetchMode;
    private $fetchTarget;
    private array $boundParams;
    // F-STMT-12: the placeholder NAME each bind was made with (":name" / "name" exactly as
    // the caller spelled it), or "" for a positional bind. $boundParams above records the
    // RESOLVED 1-based driver slot, which is all execute() needs but destroys the name
    // debugDumpParams() has to print ("Key: Name: [9] :calories"). Kept as a fourth parallel
    // array — appended and cleared in lockstep with the other three — rather than folded into
    // one array of records, because a per-bind array-of-arrays is exactly the heterogeneous
    // Mixed shape that miscompiles here.
    private array $boundNames;
    private array $boundValues;
    private array $boundTypes;
    // F-STMT-12: the PDO::PARAM_* type php-src would REPORT for each bind, which is not
    // always the one elephc dispatches on ($boundTypes above). bindValue() records the
    // caller's raw $type in both. execute($params) is where they part: php-src's
    // pdo_stmt_bind_input_params stamps EVERY element of that array PDO_PARAM_STR (2) —
    // regardless of the PHP value's type — while $boundTypes has to keep the per-value
    // dispatch tag (1 int/bool, 0 null, 2 string, 100 = internal float marker) so a later
    // no-arg execute() re-binds each value with the right driver call. Only
    // debugDumpParams() reads this array; nothing binds from it.
    private array $boundPhpTypes;
    private int $fetchColumn;
    private int $rowCount;
    private $iterRow;
    private int $iterKey;
    private bool $executed;
    // P1-4: mirrors php-src's pdo_sqlite `pre_fetched` flag — execute() eagerly
    // steps a SELECT-style statement once (see execute()'s comment) so
    // getColumnMeta() called before any explicit fetch() reports the real
    // column types of the first row instead of "no row yet". $pendingStep
    // caches that first step's result (elephc_pdo_step()'s return code) so the
    // FIRST subsequent stepCursor() call (from fetch()/fetchColumn()/etc.)
    // consumes it instead of stepping again, which would otherwise skip row 1.
    private bool $hasPendingStep;
    private int $pendingStep;
    // F-STMT-13: php-src makes $queryString read-only through a custom property-write
    // handler (dbstmt_prop_write: `zend_throw_error(NULL, "Property queryString is read
    // only")`), so `$stmt->queryString = 'x'` is an Error, not a silent overwrite of the
    // SQL the object reports. elephc has no property-write hook, but it DOES have
    // `readonly`: assignable once from the declaring class's constructor (the only place
    // this is written — see __construct), rejected everywhere else. The SQL a statement
    // reports can therefore never be overwritten, which is the point of the finding.
    //
    // HOW THE REJECTION SURFACES IS RECEIVER-TYPE DEPENDENT — verified by running both
    // shapes, not assumed (pinned by test_pdo_statement_query_string_is_readonly):
    //
    //   * Receiver narrowed to a concrete PDOStatement (e.g. behind an
    //     `instanceof PDOStatement` guard): a catchable `Error` is raised, matching php.
    //     NEAR-PARITY on the text only — elephc raises PHP's generic readonly message
    //     ("Cannot modify readonly property PDOStatement::$queryString"), not pdo_stmt.c's
    //     custom "Property queryString is read only". Same class, same catchability.
    //   * Receiver left as the `PDOStatement|bool` union that prepare()/query() return
    //     (the common shape): the write is SILENTLY DROPPED — no Error, but the property
    //     keeps its constructor value. This is a PRE-EXISTING COMPILER LIMITATION, not a
    //     PDO one: a readonly write through a union-typed receiver is not checked at all
    //     (reproduced on a plain user class whose factory returns `Box|bool`). php would
    //     throw here. The value is still protected; only the diagnostic is missing.
    public readonly string $queryString;
    // P1-11 (best-effort): mirrors PDO::$stringifyFetches, snapshotted at
    // prepare() time via setStringifyFetches(). Applied in columnValue() so
    // every fetch path (assoc/num/both/named/obj/class/key-pair/fetchColumn)
    // honors it from one place.
    private bool $stringifyFetches;
    // P1-i: mirrors PDO::ATTR_EMULATE_PREPARES, snapshotted at prepare() time
    // from the owning connection's stored value (see setEmulatePrepares()).
    // getAttribute() answers this one attribute from the snapshot instead of
    // raising IM001 like every other unsupported statement attribute — no real
    // per-statement attribute store exists any more (setAttribute() always
    // fails; see its own comment).
    private bool $emulatePrepares;
    // P2-e: mirrors PDO::ATTR_CASE / ATTR_ORACLE_NULLS, snapshotted at prepare()
    // time via setAttrCase()/setOracleNulls() (see PDO::prepare()). Applied in
    // columnName()/columnValue() so every fetch path (assoc/num/both/named/obj/
    // class/into/key-pair/fetchColumn) honors them from one place.
    private int $attrCase;
    private int $oracleNulls;
    // P1-j: roots the owning PDO object (and, transitively, its bridge
    // connection) for as long as this statement is reachable. `$conn` above is
    // a bare integer handle into the bridge's connection table — it carries no
    // reference of its own — so a statement returned out of the scope that
    // opened its connection (e.g. `return $db->query(...)` from inside a
    // function whose local `$db` then goes out of scope) would otherwise leave
    // `$conn` dangling once the PDO object is collected. A plain object-typed
    // property is enough for elephc's refcounting GC to keep the referenced
    // PDO (and its connection) alive; see setOwner(), called from
    // PDO::prepare(). PDO does not hold a reference back to any of its
    // statements, so this creates no reference cycle.
    private ?PDO $owner;

    public function __construct(int $handle, int $connection, int $errMode = 2, string $query = "") {
        // P2-o: php-src's PDOStatement constructor throws "You should not
        // create a PDOStatement manually" when invoked directly rather than
        // via PDO::prepare()/PDO::query() (its internal check is that the
        // statement has no owning `dbh` yet). elephc's constructor is
        // necessarily public — PDO::prepare() constructs this class from a
        // different class — and takes bare integer handles with no access
        // control to lean on, so the closest honest equivalent is rejecting a
        // $connection that is not a real, currently-open connection handle:
        // elephc_pdo_driver_name() returns "" for an unknown id, which is
        // exactly what a hand-constructed call passing an arbitrary/guessed
        // integer hits, since no valid handle is ever exposed to PHP code.
        // This does not catch a caller who happens to guess a live handle
        // (elephc's handles are small sequential integers), but neither would
        // any check short of real access control.
        if (elephc_pdo_driver_name($connection) === "") {
            throw new PDOException("You should not create a PDOStatement manually");
        }
        $this->stmt = $handle;
        $this->conn = $connection;
        $this->errMode = $errMode;
        // PHP exposes the prepared SQL as the public PDOStatement::$queryString
        // property; thread it through from prepare() so debugDumpParams and callers
        // can read it.
        $this->queryString = $query;
        $this->fetchMode = 4;
        $this->fetchTarget = null;
        $this->boundParams = [];
        $this->boundNames = [];
        $this->boundValues = [];
        $this->boundTypes = [];
        $this->boundPhpTypes = [];
        $this->fetchColumn = 0;
        $this->rowCount = 0;
        // Initialized to null (not false) so the inferred property type widens to
        // Mixed when rewind()/next() assign a fetched row; a bool initializer would
        // pin the type to bool and coerce stored rows away. rewind() always runs
        // before the first valid() check, so the initial value is never observed.
        $this->iterRow = null;
        $this->iterKey = 0;
        // Guards fetch*() against stepping a never-executed statement (which would
        // silently run the query with NULL binds). Set true by execute(), cleared
        // by closeCursor().
        $this->executed = false;
        $this->hasPendingStep = false;
        $this->pendingStep = 0;
        $this->stringifyFetches = false;
        $this->emulatePrepares = false;
        $this->attrCase = 0;
        $this->oracleNulls = 0;
        $this->owner = null;
    }

    // P1-j: called by PDO::prepare() with $this right after construction, so
    // the statement roots its owning connection for its whole lifetime (see
    // the $owner property comment above).
    public function setOwner(PDO $owner): void {
        $this->owner = $owner;
    }

    public function setStringifyFetches(bool $on): void {
        $this->stringifyFetches = $on;
    }

    public function setEmulatePrepares(bool $on): void {
        $this->emulatePrepares = $on;
    }

    public function setAttrCase(int $mode): void {
        $this->attrCase = $mode;
    }

    public function setOracleNulls(int $mode): void {
        $this->oracleNulls = $mode;
    }

    private function fail(string $message): void {
        // Per-statement error state (W1): the SQLSTATE, native code, and message
        // are read from the statement's own error slots and attached to errorInfo.
        if ($this->errMode == 0) {
            return;
        }
        $_sqlstate = elephc_pdo_stmt_sqlstate($this->stmt);
        if ($this->errMode == 2) {
            $_native = elephc_pdo_stmt_errcode($this->stmt);
            throw new PDOException("SQLSTATE[" . $_sqlstate . "]: " . $message, [$_sqlstate, $_native, $message]);
        }
        fwrite(STDERR, "PDO error: SQLSTATE[" . $_sqlstate . "]: " . $message . "\n");
    }

    // A synthetic (non-driver) statement-level error, e.g. IM001 "driver doesn't
    // support ..." or the FETCH_KEY_PAIR column-count check — mirrors php-src's
    // `pdo_raise_impl_error`, which writes a caller-given SQLSTATE rather than
    // reading the driver's live error state (there was no real query failure to
    // read one from). Still fully errMode-aware like fail() above: EXCEPTION
    // throws, WARNING writes to stderr, SILENT is quiet — every case leaves the
    // caller to return its own failure value.
    private function failCode(string $sqlstate, string $message): void {
        if ($this->errMode == 0) {
            return;
        }
        if ($this->errMode == 2) {
            throw new PDOException("SQLSTATE[" . $sqlstate . "]: " . $message, [$sqlstate, null, $message]);
        }
        fwrite(STDERR, "PDO error: SQLSTATE[" . $sqlstate . "]: " . $message . "\n");
    }

    public function errorCode(): ?string {
        // The 5-character SQLSTATE for the statement's last operation.
        return elephc_pdo_stmt_sqlstate($this->stmt);
    }

    public function errorInfo(): array {
        // Per-statement [SQLSTATE, native, message], mirroring PDO::errorInfo().
        $_sqlstate = elephc_pdo_stmt_sqlstate($this->stmt);
        if ($_sqlstate === "00000") {
            return ["00000", null, null];
        }
        return [$_sqlstate, elephc_pdo_stmt_errcode($this->stmt), elephc_pdo_stmt_errmsg($this->stmt)];
    }

    // P3: propagates ATTR_DEFAULT_FETCH_MODE to a freshly prepared statement,
    // mirroring php-src's OWN mechanism exactly (verified against pdo_dbh.c:
    // `stmt->default_fetch_type = dbh->default_fetch_type;` — a raw field
    // copy at statement construction, never routed through
    // pdo_stmt_setup_fetch_mode/pdo_stmt_verify_mode at all). This must stay a
    // separate, unvalidated setter rather than calling the public
    // setFetchMode() below: checkDefaultFetchMode() only rejects
    // FETCH_USE_DEFAULT (0), so a bare FETCH_CLASS/FETCH_INTO/FETCH_FUNC is a
    // legal STORED default in both php-src and this prelude (P3 relaxed the
    // former two to match php-src; FETCH_FUNC was never restricted here
    // either). A call through setFetchMode()'s OWN validation (the
    // ArgumentCountError-equivalent / FETCH_FUNC checks a few lines down)
    // would wrongly reject that otherwise-legal stored default the moment ANY
    // statement on the connection is prepared — php-src only re-validates a
    // defaulted mode lazily, when fetch()/fetchAll() actually resolves
    // PDO_FETCH_USE_DEFAULT (see fetch()'s own
    // `if ($mode == 0) { $mode = $this->fetchMode; }` resolution above, which
    // already re-runs the FETCH_FUNC/FETCH_LAZY checks at that later point).
    public function setDefaultFetchMode(int $mode): void {
        $this->fetchMode = $mode;
    }

    // F-STMT-17: names the offending value the way php-src's zend_zval_value_name() does in
    // an argument TypeError. It is a near-copy of PDO::attrValueTypeName() (see the
    // F-CORE-03 comment there) rather than a call to it: that one is `private` on a
    // DIFFERENT class, and this prelude has no trait or shared-private mechanism to reach it
    // from here — promoting it to `public static` on PDO would bolt a method onto PDO's
    // public surface that real PHP does not have, a worse divergence than a short duplicate.
    //
    // It is NOT a byte-for-byte copy: zend_zval_value_name() spells a bool as "true"/"false"
    // (PHP 8.3+), which is what real PHP prints here — verified against php 8.x:
    // `setFetchMode(PDO::FETCH_COLUMN, true)` says "must be of type int, true given". The
    // PDO-side copy still says "bool"; that is a pre-existing text divergence in the
    // attribute TypeErrors, left alone here because its messages are pinned by tests.
    // The one approximation left: php names an OBJECT by its class, this reports "object".
    private function argValueTypeName(mixed $value): string {
        if (is_int($value)) {
            return "int";
        }
        if (is_bool($value)) {
            // Explicit (bool) cast rather than a bare `if ($value)`: the value is a Mixed
            // parameter, and every other truthiness test in this prelude casts first.
            if ((bool) $value) {
                return "true";
            }
            return "false";
        }
        if (is_float($value)) {
            return "float";
        }
        if (is_string($value)) {
            return "string";
        }
        if (is_array($value)) {
            return "array";
        }
        if (is_null($value)) {
            return "null";
        }
        return "object";
    }

    public function setFetchMode(int $mode, mixed $classOrColumn = null): bool {
        // P2-d: reject an out-of-range base mode and a negative FETCH_COLUMN
        // index BEFORE storing anything (mirrors php-src's pdo_stmt_verify_mode /
        // pdo_stmt_setup_fetch_mode ValueErrors), so a rejected call leaves the
        // statement's previous fetch mode untouched. OR-able high-bit flags (e.g.
        // FETCH_GROUP, FETCH_CLASSTYPE) are masked off first, matching fetch()'s
        // own `$mode & 0xFFFF` base-mode masking; 0..12 covers every FETCH_*
        // base mode this prelude defines (FETCH_DEFAULT..FETCH_KEY_PAIR).
        $_base = $mode & 0xFFFF;
        if ($_base < 0 || $_base > 12) {
            throw new ValueError("PDOStatement::setFetchMode(): Argument #1 (\$mode) must be a bitmask of PDO::FETCH_* constants");
        }
        // P3: php-src's pdo_stmt_setup_fetch_mode calls pdo_stmt_verify_mode
        // with fetch_all=false for setFetchMode(), which rejects FETCH_FUNC
        // outright (it is valid only as fetchAll()'s first argument) — the
        // exact same ValueError text fetch()'s own FETCH_FUNC check above
        // throws (verified against php-src: both call sites hit the identical
        // `case PDO_FETCH_FUNC: if (!fetch_all) { zend_value_error(...); }`).
        if ($_base == 10) {
            throw new ValueError("Can only use PDO::FETCH_FUNC in PDOStatement::fetchAll()");
        }
        // F-STMT-09: every gate below tests $_base, the FLAG-MASKED mode — they used to
        // test the RAW $mode, which is false the moment ANY high-bit flag is OR-ed in.
        // `setFetchMode(PDO::FETCH_CLASS|PDO::FETCH_PROPS_LATE, 'Row')` therefore matched
        // no gate at all: the arity checks were skipped AND the class name was dropped on
        // the floor by the storage block at the bottom, leaving a statement in FETCH_CLASS
        // mode with no target — which then silently fetched stdClass rows.
        // F-STMT-17: php-src checks the column argument's TYPE before its RANGE
        // (pdo_stmt.c's PDO_FETCH_COLUMN arm: `if (Z_TYPE(args[0]) != IS_LONG) {
        // zend_argument_type_error(2, "must be of type int, %s given", ...); }` immediately
        // ahead of the `< 0` value check below). The argument is variadic `mixed ...$args`
        // in the stub, so it is NEVER juggled: a bool, a float, and even the numeric string
        // "3" are all IS_LONG-mismatches and all raise the TypeError — hence the strict
        // is_int() here rather than an is_numeric()-style shape test. This prelude used to
        // fall straight into the `(int) $classOrColumn` cast below, and `(int) "abc"` is 0,
        // so `setFetchMode(PDO::FETCH_COLUMN, "abc")` silently selected column 0 and
        // reported success.
        //
        // The message carries NO argument NAME — "Argument #2 must be of type int, string
        // given" — because zend never names a variadic parameter in an argument error
        // (verified against real php: `Argument #2 must be of type int, string given`, and
        // likewise `Argument #2 must be greater than or equal to 0` for the range error
        // below). FOLLOW-UP, deliberately not fixed here: the neighbouring ValueError texts
        // in this method DO spell an `($args)` php never prints. Their exact strings are
        // pinned by existing tests, so correcting them is a test-touching change and out of
        // scope for this one.
        if ($_base == 7 && $classOrColumn !== null && !is_int($classOrColumn)) {
            throw new TypeError("PDOStatement::setFetchMode(): Argument #2 must be of type int, " . $this->argValueTypeName($classOrColumn) . " given");
        }
        if ($_base == 7 && $classOrColumn !== null && ((int) $classOrColumn) < 0) {
            throw new ValueError("PDOStatement::setFetchMode(): Argument #2 (\$args) must be greater than or equal to 0");
        }
        // F-STMT-09: FETCH_CLASSTYPE reads the class name from COLUMN 0'S VALUE at fetch
        // time (see fetch()'s own CLASSTYPE branch), so an explicit class argument is not
        // merely redundant — it is a contradiction, and php-src rejects the combination
        // outright (pdo_stmt.c:1783-1790: the CLASSTYPE arm of the FETCH_CLASS case takes
        // its class from the data and raises zend_argument_count_error the moment a
        // variadic class argument accompanies it). This prelude used to accept the combo
        // and quietly discard the argument. Same ArgumentCountError-vs-ValueError
        // substitution as the arity gates below (elephc has no ArgumentCountError class),
        // with php-src's literal message text.
        if ($_base == 8 && ($mode & 0x40000) != 0 && $classOrColumn !== null) {
            throw new ValueError("PDOStatement::setFetchMode() expects exactly 1 argument for the fetch mode provided, 2 given");
        }
        // P3: php-src's pdo_stmt_setup_fetch_mode raises an ArgumentCountError
        // when FETCH_COLUMN/FETCH_CLASS/FETCH_INTO is given with no further
        // argument at all (verified against php-src's exact wording: "%s()
        // expects exactly/at least %d arguments for the fetch mode provided,
        // %d given", %s = "PDOStatement::setFetchMode", the argument count
        // derived from this method's own arg positions). elephc has no
        // ArgumentCountError class (not part of its builtin exception
        // hierarchy) and, unlike real PHP's variadic-arity introspection,
        // cannot distinguish "argument omitted" from "argument explicitly
        // null" on a plain `$classOrColumn = null` default parameter — so this
        // raises the closest available ValueError (still catchable via
        // `\Error`, just not via a real `\ArgumentCountError`) with php-src's
        // literal message text for the omitted case.
        if ($_base == 7 && $classOrColumn === null) {
            throw new ValueError("PDOStatement::setFetchMode() expects exactly 2 arguments for the fetch mode provided, 1 given");
        }
        // FETCH_CLASS is the one base mode whose class argument is OPTIONAL — but only
        // under CLASSTYPE, which supplies it from the data instead (and which the gate
        // above has already proven was NOT accompanied by an explicit one).
        if ($_base == 8 && ($mode & 0x40000) == 0 && $classOrColumn === null) {
            throw new ValueError("PDOStatement::setFetchMode() expects at least 2 arguments for the fetch mode provided, 1 given");
        }
        if ($_base == 9 && $classOrColumn === null) {
            throw new ValueError("PDOStatement::setFetchMode() expects exactly 2 arguments for the fetch mode provided, 1 given");
        }
        $this->fetchMode = $mode;
        if ($_base == 7 && $classOrColumn !== null) {
            $this->fetchColumn = (int) $classOrColumn;
        } elseif (($_base == 8 || $_base == 9) && $classOrColumn !== null) {
            $this->fetchTarget = $classOrColumn;
        }
        return true;
    }

    public function bindValue($parameter, $value, int $type = 2): bool {
        // F-STMT-05: php-src's PHP_METHOD(PDOStatement, bindValue) validates the
        // parameter identifier BEFORE recording anything — a positional slot below 1
        // is a ValueError ("must be greater than or equal to 1"), and an empty named
        // placeholder is zend_argument_must_not_be_empty_error(1). This prelude used
        // to cast blindly and report success for both, so `bindValue(0, 'x')` bound
        // nothing and said it had worked.
        if (is_int($parameter)) {
            if (((int) $parameter) < 1) {
                throw new ValueError("PDOStatement::bindValue(): Argument #1 (\$param) must be greater than or equal to 1");
            }
        } elseif (((string) $parameter) === "") {
            throw new ValueError("PDOStatement::bindValue(): Argument #1 (\$param) must not be empty");
        }
        // Resolve the 1-based slot index now and record it. The named-placeholder
        // lookup must not be interleaved with value binds in execute()'s loop: a
        // loop body that branches between "lookup index" and "no lookup" corrupts
        // a sibling bind in generated code. Recording resolved int slots keeps
        // execute()'s bind loop uniform. F-STMT-12: the caller's spelling of the
        // placeholder is recorded alongside it ("" for a positional bind) — the resolved
        // slot alone cannot reproduce debugDumpParams()'s "Key: Name:" block.
        if (is_int($parameter)) {
            $_slot = (int) $parameter;
            $_pname = "";
        } else {
            $_slot = (int) elephc_pdo_bind_parameter_index($this->stmt, (string) $parameter);
            $_pname = (string) $parameter;
        }
        $this->boundParams[] = $_slot;
        $this->boundNames[] = $_pname;
        $this->boundValues[] = $value;
        $this->boundTypes[] = $type;
        // F-STMT-12: php-src reports a bindValue()/bindParam() bind with the type the
        // caller passed, flags and all (param->param_type is stored verbatim) — so the
        // reported type and the dispatch type are the same value on this path.
        $this->boundPhpTypes[] = $type;
        return true;
    }

    public function bindParam($parameter, $variable, int $type = 2, int $maxLength = 0, mixed $driverOptions = null): bool {
        // F-STMT-05: php-src validates bindParam()'s own Argument #1 exactly as it
        // validates bindValue()'s, so the guard is repeated here rather than left to
        // the bindValue() delegation below — otherwise the ValueError would name the
        // wrong method.
        if (is_int($parameter)) {
            if (((int) $parameter) < 1) {
                throw new ValueError("PDOStatement::bindParam(): Argument #1 (\$param) must be greater than or equal to 1");
            }
        } elseif (((string) $parameter) === "") {
            throw new ValueError("PDOStatement::bindParam(): Argument #1 (\$param) must not be empty");
        }
        // Unlike PHP, the value is recorded now (not read by reference at execute
        // time): bind right before execute(), or use bindValue(). $maxLength (the
        // LOB/output-buffer length hint) and $driverOptions are accepted for
        // signature compatibility with the common
        // `bindParam($p, $v, PDO::PARAM_STR, 4000)` idiom but not applied — the
        // bind loop in execute() has no by-reference length cap or driver-option
        // channel to feed them into.
        $_unusedMaxLength = $maxLength;
        $_unusedDriverOptions = $driverOptions;
        return $this->bindValue($parameter, $variable, $type);
    }

    public function bindColumn(string|int $column, mixed &$var, int $type = 2, int $maxLength = 0, mixed $driverOptions = null): bool {
        // F-STMT-05: php-src validates bindColumn()'s Argument #1 with the same two
        // checks bindValue()/bindParam() get, and it does so during parameter
        // validation — i.e. AHEAD of any driver dispatch. So the ValueError must win
        // over the not-supported PDOException below, keeping the failure a caller
        // sees for a malformed argument identical to real PHP's.
        if (is_int($column) && ((int) $column) < 1) {
            throw new ValueError("PDOStatement::bindColumn(): Argument #1 (\$column) must be greater than or equal to 1");
        }
        if (is_string($column) && ((string) $column) === "") {
            throw new ValueError("PDOStatement::bindColumn(): Argument #1 (\$column) must not be empty");
        }
        // P0-4: NOT SUPPORTED — fails loudly rather than silently accepting the
        // binding and doing nothing. PHP's bindColumn() stores a reference to
        // $var and writes each fetched column into it on every subsequent
        // fetch(PDO::FETCH_BOUND). Storing that "escaping" reference needs a
        // by-reference parameter to be assignable into an object property
        // (`$this->boundColumns[$column] = &$var;`) so a later fetch() call can
        // still reach it; that assignment form does not even parse in elephc
        // (confirmed directly: `$this->x = &$v;` fails with "Unexpected token:
        // Ampersand" before the checker ever runs), and no other by-ref-capture
        // mechanism exists in this compiler's PHP subset. The parameter is kept
        // by-reference here (matching PHP's real signature) only for call-site
        // compatibility; the value is never read.
        $_unusedColumn = $column;
        $_unusedType = $type;
        $_unusedMaxLength = $maxLength;
        $_unusedDriverOptions = $driverOptions;
        throw new PDOException("PDOStatement::bindColumn() is not supported");
    }

    public function execute(?array $params = null): bool {
        $this->executed = true;
        elephc_pdo_reset($this->stmt);
        elephc_pdo_clear_bindings($this->stmt);
        // F-STMT-06 / F-PARSE-06: neither replay loop below used to check the
        // resolved slot index OR any elephc_pdo_bind_* return code, so a named
        // placeholder the prepared SQL never declares (bind_parameter_index()
        // returns 0 for "unknown") and an out-of-range positional slot (every
        // bind_* returns 0 there — the driver's own bounds check / SQLITE_RANGE)
        // both bound NOTHING while execute() reported success, silently dropping
        // the value. php-src raises HY093 for both. Each loop records the failure
        // in $_bindError and breaks; it is reported once past the branch, so the
        // errMode-aware error path is shared and neither loop body has to unwind
        // out of its own iteration.
        $_bindError = "";
        // P1-c: php-src's PHP_METHOD(PDOStatement, execute) REPLACES the bound
        // parameters with $input_params when it is given — it never layers the
        // call-time array on top of earlier bindValue()/bindParam() bindings, so
        // a slot bound earlier but absent from $params must NOT keep its stale
        // value. Hence these two branches are mutually exclusive: the recorded
        // bindValue()/bindParam() bindings replay ONLY when no $params array is
        // given at all.
        if ($params === null) {
            // Apply bindValue()/bindParam() bindings recorded since construction
            // (or, per the P2 comment below, the last execute($params) array).
            // Slots are already resolved to ints, so this loop never looks up an
            // index (keeping the body uniform across positional and named binds).
            $_count = count($this->boundParams);
            for ($_i = 0; $_i < $_count; $_i++) {
                $_slot = (int) $this->boundParams[$_i];
                $_value = $this->boundValues[$_i];
                // F-STMT-08: php-src ALWAYS reduces a bound type to its base type
                // before dispatching on it — PDO_PARAM_TYPE(x) is
                // `((x) & ~PDO_PARAM_FLAGS)` with PDO_PARAM_FLAGS = 0xFFFF0000, the
                // high half where PARAM_INPUT_OUTPUT (0x80000000), PARAM_STR_NATL
                // (0x40000000) and PARAM_STR_CHAR (0x20000000) live. Dispatching on
                // the RAW value made `PDO::PARAM_INT|PDO::PARAM_INPUT_OUTPUT` match
                // no branch at all and fall through to the generic TEXT one, binding
                // an int as a string. The raw value stays in $this->boundTypes (it is
                // what a caller bound); only the dispatch is masked. Same `& 0xFFFF`
                // base-mode idiom the fetch-mode paths already use.
                $_btype = ((int) $this->boundTypes[$_i]) & 0xFFFF;
                if ($_slot < 1) {
                    // bindValue()/bindParam() now reject a positional slot below 1 up
                    // front, so a slot of 0 reaching here can only be a NAMED
                    // placeholder that bind_parameter_index() could not resolve —
                    // php-src's "parameter was not defined" flavor of HY093.
                    $_bindError = "Invalid parameter number: parameter was not defined";
                    break;
                }
                $_brc = 0;
                if ($_btype == 0 || is_null($_value)) {
                    $_brc = elephc_pdo_bind_null($this->stmt, $_slot);
                } elseif ($_btype == 1) {
                    $_brc = elephc_pdo_bind_int($this->stmt, $_slot, (int) $_value);
                } elseif ($_btype == 5) {
                    // F-STMT-07: PDO::PARAM_BOOL gets the driver's own boolean bind
                    // (php-src's PDO_PARAM_BOOL case) instead of being folded into
                    // PARAM_INT — that is what makes PostgreSQL send a real 't'/'f'
                    // for a BOOL column rather than an integer literal it will refuse.
                    // The value is truthiness-reduced first, mirroring the zval_is_true()
                    // php-src applies to this parameter type (so a bound `5` binds
                    // true, not 5). SQLite/MySQL bind it as 0/1, exactly as before.
                    $_bval = ((bool) $_value) ? 1 : 0;
                    $_brc = elephc_pdo_bind_bool($this->stmt, $_slot, $_bval);
                } elseif ($_btype == 3) {
                    // PDO::PARAM_LOB: route through bind_blob (raw bytes, embedded
                    // NUL preserved) rather than bind_text.
                    $_s = (string) $_value;
                    $_brc = elephc_pdo_bind_blob($this->stmt, $_slot, $_s, strlen($_s));
                } elseif ($_btype == 100) {
                    // P2 (not a real PDO::PARAM_* value): an internal marker
                    // recorded only by execute($params)'s array-bind rebuild
                    // below, for a PHP float element, so a later no-arg
                    // execute() replay re-binds it as a double instead of
                    // falling into the text branch and stringifying it.
                    $_brc = elephc_pdo_bind_double($this->stmt, $_slot, (float) $_value);
                } else {
                    // PDO::PARAM_STR (and anything else): bind_text with the
                    // measured byte length so an embedded NUL byte survives.
                    $_s = (string) $_value;
                    $_brc = elephc_pdo_bind_text($this->stmt, $_slot, $_s, strlen($_s));
                }
                if ($_brc == 0) {
                    // The slot resolved but the driver refused it: an out-of-range
                    // positional index (e.g. bindValue(5, ...) on a 2-placeholder
                    // statement), which is php-src's bare "Invalid parameter number".
                    $_bindError = "Invalid parameter number";
                    break;
                }
            }
        } else {
            // P2: php-src's pdo_stmt_bind_input_params DESTROYS
            // stmt->bound_params and REBUILDS it from $input_params, so a
            // LATER no-arg execute() replays THIS array, not whatever
            // bindValue()/bindParam() calls preceded it (verified against
            // php-src: `bindValue(1,'a'); execute(['b']); execute();` inserts
            // 'b' on BOTH calls in real PHP). Clear the recorded-bind
            // bookkeeping and rebuild it below from $params, in lockstep with
            // the driver binds, so the replay loop above sees exactly this
            // call's array on a subsequent no-arg execute().
            $this->boundParams = [];
            $this->boundNames = [];
            $this->boundValues = [];
            $this->boundTypes = [];
            $this->boundPhpTypes = [];
            // Apply this call's parameter array (positional ? and named :name).
            foreach ($params as $_key => $_pv) {
                if (is_int($_key)) {
                    $_idx = $_key + 1;
                    // F-STMT-12: no name for a positional element, exactly as php-src's
                    // pdo_stmt_bind_input_params leaves param->name NULL for an integer key.
                    $_pname = "";
                } else {
                    $_idx = elephc_pdo_bind_parameter_index($this->stmt, (string) $_key);
                    // php-src records the array key VERBATIM (with or without its leading
                    // colon — it tries both spellings when binding), so record it verbatim.
                    $_pname = (string) $_key;
                }
                $_pslot = (int) $_idx;
                if ($_pslot < 1) {
                    // F-STMT-06: the same unresolvable-name case as the replay loop
                    // above — an `execute([':nope' => 1])` key the prepared SQL does
                    // not declare resolves to slot 0 and used to vanish silently.
                    $_bindError = "Invalid parameter number: parameter was not defined";
                    break;
                }
                $_prc = 0;
                if (is_int($_pv)) {
                    $_prc = elephc_pdo_bind_int($this->stmt, $_pslot, (int) $_pv);
                    $this->boundTypes[] = 1;
                } elseif (is_bool($_pv)) {
                    $_prc = elephc_pdo_bind_int($this->stmt, $_pslot, (int) $_pv);
                    $this->boundTypes[] = 1;
                } elseif (is_float($_pv)) {
                    $_prc = elephc_pdo_bind_double($this->stmt, $_pslot, (float) $_pv);
                    // 100: see the replay loop's matching comment above.
                    $this->boundTypes[] = 100;
                } elseif (is_null($_pv)) {
                    $_prc = elephc_pdo_bind_null($this->stmt, $_pslot);
                    $this->boundTypes[] = 0;
                } else {
                    // The array-bind path carries no PDO type, so PARAM_STR /
                    // length-safe TEXT (embedded NUL preserved) is correct here.
                    $_ps = (string) $_pv;
                    $_prc = elephc_pdo_bind_text($this->stmt, $_pslot, $_ps, strlen($_ps));
                    $this->boundTypes[] = 2;
                }
                $this->boundParams[] = $_pslot;
                $this->boundNames[] = $_pname;
                $this->boundValues[] = $_pv;
                // F-STMT-12: php-src stamps PDO_PARAM_STR (2) on every element of an
                // execute($params) array, whatever the PHP value's type — verified against
                // real PHP 8.x: `execute([1])` then debugDumpParams() prints param_type=2
                // for the integer. The dispatch tag recorded in $boundTypes above (1/0/100)
                // is elephc-internal and must NOT leak into the dump.
                $this->boundPhpTypes[] = 2;
                if ($_prc == 0) {
                    // F-PARSE-06: a positional key past the placeholder count (the
                    // array is 0-based, the slot 1-based) — php-src's bare
                    // "Invalid parameter number".
                    $_bindError = "Invalid parameter number";
                    break;
                }
            }
        }
        if ($_bindError !== "") {
            // Nothing has been run on the driver yet, so the statement is NOT
            // executed — clear the flag set at the top of this method so a later
            // fetch() cannot step a statement whose binds were rejected. failCode()
            // is errMode-aware exactly like every other statement failure
            // (EXCEPTION throws, WARNING warns, SILENT is quiet); all three modes
            // return false from execute() rather than reporting a phantom success.
            $this->executed = false;
            $this->failCode("HY093", $_bindError);
            return false;
        }
        // A statement with no result columns (INSERT/UPDATE/DELETE/DDL) is run
        // now.
        if (elephc_pdo_column_count($this->stmt) == 0) {
            $_step = elephc_pdo_step($this->stmt);
            if ($_step < 0) {
                $this->fail(elephc_pdo_errmsg($this->conn));
                $this->rowCount = elephc_pdo_changes($this->conn);
                return false;
            }
        } else {
            // P1-4: a SELECT-style statement (column_count > 0) is pre-stepped
            // right here, mirroring php-src's pdo_sqlite `pre_fetched` behavior
            // (`pdo_sqlite_stmt_execute` steps unconditionally, regardless of
            // statement shape). This makes getColumnMeta() report the real
            // column types of the first row even before the caller's first
            // explicit fetch(); a fetch() call with no prior stepCursor()
            // consumption still sees exactly that first row (see
            // stepCursor()), so no row is skipped. A genuine error on this
            // first step (e.g. a constraint violation on `INSERT ... RETURNING`)
            // fails execute() itself here, exactly like the no-result-columns
            // branch above — matching real sqlite, where the very first step
            // is where such errors actually surface.
            $this->pendingStep = elephc_pdo_step($this->stmt);
            $this->hasPendingStep = true;
            if ($this->pendingStep < 0) {
                $this->fail(elephc_pdo_errmsg($this->conn));
                $this->rowCount = elephc_pdo_changes($this->conn);
                return false;
            }
        }
        // Snapshot the affected-row count now, so rowCount() reports this
        // statement's result even if another statement runs on the same
        // connection afterward. The bridge's changes() is connection-wide, so
        // reading it lazily in rowCount() would otherwise return a later
        // statement's count (e.g. PostgreSQL/MySQL overwrite changes() with a
        // SELECT's row count).
        $this->rowCount = elephc_pdo_changes($this->conn);
        // P1-2: real pdo_sqlite always reports rowCount()==0 after a
        // column-returning (SELECT-style) statement — sqlite3_changes() is a
        // write-count, connection-wide, and unrelated to a SELECT's own results
        // even once the SELECT has been (pre-)stepped above, so it would
        // otherwise echo an EARLIER statement's write count, e.g. 3 after three
        // prior INSERTs. PostgreSQL/MySQL are unaffected: they materialize the
        // whole result set above and legitimately set changes() to this
        // SELECT's own row count.
        if (elephc_pdo_column_count($this->stmt) > 0 && elephc_pdo_driver_name($this->conn) === "sqlite") {
            $this->rowCount = 0;
        }
        return true;
    }

    private function columnValue(int $index): mixed {
        $_type = elephc_pdo_column_type($this->stmt, $index);
        if ($_type == 1) {
            $_intVal = elephc_pdo_column_int($this->stmt, $index);
            if ($this->stringifyFetches) {
                return (string) $_intVal;
            }
            return $_intVal;
        } elseif ($_type == 2) {
            $_dblVal = elephc_pdo_column_double($this->stmt, $index);
            if ($this->stringifyFetches) {
                return (string) $_dblVal;
            }
            return $_dblVal;
        } elseif ($_type == 5) {
            // NULL is never stringified, matching PHP. P2-e: ATTR_ORACLE_NULLS's
            // NULL_TO_STRING (2) converts it to "" here, mirroring php-src's
            // fetch_value() (its final `oracle_nulls == PDO_NULL_TO_STRING` check).
            if ($this->oracleNulls == 2) {
                return "";
            }
            return null;
        }
        // F-QUAL-01: TEXT/BLOB values are copied out of the bridge in ONE call. This
        // is the dispatch point for every fetch path (assoc/num/both/named/obj/class/
        // into/key-pair/fetchColumn), and it used to loop over column_data_byte once
        // per byte — N FFI calls, each locking and unlocking the bridge's statement
        // table, plus N string concatenations, so an N-byte column cost O(N) FFI and
        // built the string in O(N^2). column_data_ptr/column_data_len are the
        // length-counted pair (they never go through the NUL-stripping store_cstr) and
        // ptr_read_string copies an EXACT byte count with no NUL-termination
        // semantics, so this stays byte-exact for values with embedded NUL bytes —
        // the sole reason the byte loop existed in the first place.
        //
        // The $_len == 0 guard is load-bearing, not cosmetic: the bridge returns a
        // NULL pointer for an empty buffer (store_bytes) and ptr_read_string fatals on
        // a NULL pointer (__rt_ptr_check_nonnull, which runs before the length is even
        // looked at), so an empty TEXT column must not reach it.
        $_len = elephc_pdo_column_data_len($this->stmt, $index);
        $_out = "";
        if ($_len > 0) {
            $_out = ptr_read_string(elephc_pdo_column_data_ptr($this->stmt, $index), $_len);
        }
        // P2-e: ATTR_ORACLE_NULLS's NULL_EMPTY_STRING (1) converts an empty
        // TEXT/BLOB value to null, mirroring php-src's fetch_value() (its
        // `IS_STRING && Z_STRLEN_P(dest) == 0` check, which runs before any
        // stringify handling there — moot here since TEXT/BLOB values are never
        // stringified by this method).
        if ($this->oracleNulls == 1 && $_out === "") {
            return null;
        }
        return $_out;
    }

    // P2-e: ATTR_CASE-aware column-name accessor — folds the raw bridge name to
    // upper/lower case per the statement's stored setting (0 = natural, no
    // change). Every branch that uses a column name as an array key or object
    // property name goes through this so the fold applies from one place
    // (FETCH_ASSOC/FETCH_NAMED/FETCH_BOTH's string-keyed half, FETCH_OBJ/
    // FETCH_CLASS/FETCH_INTO via assignColumns(), and getColumnMeta()'s "name"
    // entry) — mirrors php-src's pdo_stmt_describe_columns(), which folds each
    // column's name once, shared by every fetch style that reads it.
    private function columnName(int $index): string {
        $_raw = elephc_pdo_column_name($this->stmt, $index);
        if ($this->attrCase == 1) {
            return strtoupper($_raw);
        }
        if ($this->attrCase == 2) {
            return strtolower($_raw);
        }
        return $_raw;
    }

    private function assignColumns(mixed $object, int $count): mixed {
        return $this->assignColumnsFrom($object, 0, $count);
    }

    // The same hydration, but starting at column $start rather than column 0 — the one
    // thing FETCH_CLASSTYPE (F-STMT-02), FETCH_GROUP and FETCH_UNIQUE (F-STMT-15) all
    // need. Each of those CONSUMES column 0 (as the class name / as the grouping key),
    // and php-src then EXCLUDES it from the row it hydrates: do_fetch() literally
    // advances its column cursor past it (`fetch_value(stmt, &val, i++, NULL)` for
    // CLASSTYPE, pdo_stmt.c:805-829; `i++` after reading the group key, pdo_stmt.c:897-909)
    // so the value that became the key never also becomes a property/element. A row whose
    // key column was silently re-assigned as data would be wrong in the way that is
    // hardest to notice — an extra property nobody asked for.
    private function assignColumnsFrom(mixed $object, int $start, int $count): mixed {
        for ($_i = $start; $_i < $count; $_i++) {
            $_value = $this->columnValue($_i);
            $_name = $this->columnName($_i);
            $object->{$_name} = $_value;
        }
        return $object;
    }

    // Advances the cursor and returns elephc_pdo_step()'s result code
    // (negative = error, 0 = no more rows, positive = a row is available).
    // Every caller that consumes rows from this statement's cursor (fetch(),
    // fetchColumn(), fetchObject(), and fetchAll()'s FETCH_KEY_PAIR loop) goes
    // through this instead of calling elephc_pdo_step() directly, so that
    // execute()'s eager pre-step (see execute()'s comment; P1-4) is consumed
    // exactly once instead of being silently skipped past.
    private function stepCursor(): int {
        if ($this->hasPendingStep) {
            $this->hasPendingStep = false;
            return $this->pendingStep;
        }
        return elephc_pdo_step($this->stmt);
    }

    // F-STMT-01: php-src's signature, restored. This method's SECOND PARAMETER USED TO BE
    // FABRICATED — a `mixed $classOrObject` that let a caller pass FETCH_CLASS's class or
    // FETCH_INTO's object straight to fetch(). Real PDO has NO such facility: the stub is
    //   fetch(int $mode = PDO::FETCH_DEFAULT,
    //         int $cursorOrientation = PDO::FETCH_ORI_NEXT,
    //         int $cursorOffset = 0): mixed
    // and position 2 is an INT ORIENTATION, so the invented idiom
    // `fetch(PDO::FETCH_CLASS, Row::class)` is a TypeError in real PHP, while the
    // LEGITIMATE `fetch($mode, PDO::FETCH_ORI_NEXT)` used to push an int into the class
    // slot. Class/object targeting is done EXCLUSIVELY through setFetchMode() beforehand
    // (or fetchObject()), so FETCH_CLASS/FETCH_INTO now read $this->fetchTarget and
    // nothing else.
    //
    // $cursorOrientation is ACCEPTED and every value is treated as FETCH_ORI_NEXT: this
    // bridge's cursors are forward-only (PDO::CURSOR_FWDONLY — no driver here opens a
    // scrollable one, and PDO::ATTR_CURSOR is inert), and php-src likewise ignores the
    // orientation on a forward-only cursor. On a CURSOR_SCROLL statement real PHP WOULD
    // honor FETCH_ORI_FIRST/LAST/PRIOR/ABS/REL (with $cursorOffset for the last two) and
    // seek accordingly; that is the divergence, and it is a property of the cursor, not
    // of this signature. Both parameters are read once here so neither trips the
    // unused-parameter warning.
    public function fetch(int $mode = 0, int $cursorOrientation = 0, int $cursorOffset = 0): mixed {
        $_unusedCursorOrientation = $cursorOrientation;
        $_unusedCursorOffset = $cursorOffset;
        if (!$this->executed) {
            return false;
        }
        if ($mode == 0) {
            $mode = $this->fetchMode;
        }
        // Separate the base fetch mode from the OR-able flags (FETCH_GROUP and
        // friends live in the high bits) and dispatch on the base, so a flagged
        // mode is not silently treated as FETCH_BOTH.
        $_base = $mode & 0xFFFF;
        // F-STMT-03: the FETCH_LAZY restriction used to be INVERTED. php-src's
        // pdo_stmt_verify_mode rejects FETCH_LAZY ONLY when fetch_all is true — i.e. only
        // in fetchAll() (where the rejection now lives) — and it WORKS in fetch(),
        // returning a lazy PDORow whose columns materialize on property access.
        //
        // elephc has no PDORow class (F-SURF-02: it is the one class of php-src's PDO
        // surface this prelude does not declare, because a lazily-materializing row object
        // needs a __get that can reach back into a live statement cursor), so fetch(LAZY)
        // cannot return the one thing it is defined to return. It therefore fails LOUDLY
        // and says exactly why, rather than substituting an eager row of some other shape
        // and letting a caller believe it got a PDORow — the class is observable
        // (`instanceof PDORow`, `$row->queryString`), so a stand-in would be a lie.
        // A PDOException (not the old ValueError) because this is an unsupported-feature
        // error, not a rejected argument — the same shape, and the same reasoning, as
        // fetchAll()'s FETCH_FUNC refusal.
        if ($_base == 1) {
            throw new PDOException("PDO::FETCH_LAZY is not supported: elephc has no PDORow class");
        }
        // P0-3: real PHP restricts FETCH_FUNC to fetchAll() and raises exactly
        // this ValueError (verified against php-src: `zend_value_error("Can
        // only use PDO::FETCH_FUNC in PDOStatement::fetchAll()")`, with no
        // "Argument #N" prefix since that helper does not add one) from
        // fetch(); fail the same way here instead of falling through to the
        // BOTH-shaped default.
        if ($_base == 10) {
            throw new ValueError("Can only use PDO::FETCH_FUNC in PDOStatement::fetchAll()");
        }
        // P1: FETCH_BOUND advances the cursor and reports whether a row was
        // available, exactly like php-src's `do_fetch` (`how == PDO_FETCH_BOUND`
        // → `RETVAL_TRUE` once the cursor has stepped, so a no-more-rows result
        // reports false through the fetch()-level "no row" path instead).
        // bindColumn()'s write-back is separately unsupported (see its own doc
        // comment); with no bound columns there is nothing further to do here.
        if ($_base == 6) {
            $_boundRc = $this->stepCursor();
            if ($_boundRc < 0) {
                $this->fail(elephc_pdo_errmsg($this->conn));
                return false;
            }
            return $_boundRc != 0;
        }
        // P1: FETCH_CLASSTYPE (class-from-first-column) is an OR-able flag bit
        // that this prelude's `& 0xFFFF` base-mode mask silently drops. Verified
        // against php-src's `pdo_stmt_verify_mode`: a base mode of FETCH_CLASS
        // jumps straight to its own switch case, skipping the CLASSTYPE check
        // entirely (so FETCH_CLASS|FETCH_CLASSTYPE is accepted), while every
        // other base mode falls into the `default:` branch, which rejects
        // CLASSTYPE with a ValueError. FETCH_PROPS_LATE (constructor-first
        // hydration order) is NEVER checked in that function at all — it is not
        // a rejection reason for any base mode — and since elephc's FETCH_CLASS
        // is already unconditionally ctor-first, honoring
        // FETCH_CLASS|FETCH_PROPS_LATE costs nothing, so it is intentionally
        // not gated here.
        if (($mode & 0x40000) != 0 && $_base != 8) {
            throw new ValueError('PDOStatement::fetch(): Argument #1 ($mode) must use PDO::FETCH_CLASSTYPE with PDO::FETCH_CLASS');
        }
        $_rc = $this->stepCursor();
        if ($_rc < 0) {
            $this->fail(elephc_pdo_errmsg($this->conn));
            return false;
        }
        if ($_rc == 0) {
            return false;
        }
        $_count = elephc_pdo_column_count($this->stmt);
        if ($_base == 7) {
            // FETCH_COLUMN: yield a single column's value as a scalar instead of a
            // row array. The column index defaults to 0 and is set via the second
            // argument to setFetchMode(PDO::FETCH_COLUMN, $col).
            return $this->columnValue($this->fetchColumn);
        }
        if ($_base == 12) {
            // FETCH_KEY_PAIR: exactly two columns map to [col0 => col1]. P2-b:
            // php-src raises this via pdo_raise_impl_error ("HY000"), which is
            // errMode-aware (SILENT/WARNING return false instead of throwing),
            // not a bare unconditional throw.
            if ($_count != 2) {
                $this->failCode("HY000", "PDO::FETCH_KEY_PAIR fetch mode requires the result set to contain exactly 2 columns.");
                return false;
            }
            $_pk = $this->columnValue(0);
            $_pv = $this->columnValue(1);
            $_pair = [];
            $_pair[$_pk] = $_pv;
            return $_pair;
        }
        if ($_base == 5) {
            // FETCH_OBJ: materialize a real stdClass and assign each column as a
            // dynamic property, preserving numeric property names and binary data.
            return $this->assignColumns(new stdClass(), $_count);
        }
        if ($_base == 8) {
            // F-STMT-02: FETCH_CLASSTYPE (0x40000) means the class name is NOT the one
            // configured on the statement — it is READ FROM COLUMN 0'S RUNTIME VALUE, row
            // by row, so one result set can hydrate a different class per row
            // (`SELECT type_col, … FROM t` with `type_col` holding 'Cat' or 'Dog').
            // php-src (pdo_stmt.c:805-829) does exactly three things this prelude used to
            // do none of: it fetch_value()s column 0, it zend_lookup_class()es that string
            // and FALLS BACK TO stdClass when no such class exists, and it then hydrates
            // from column 1 onward — column 0 was CONSUMED as the type tag and must not
            // also land in a property. The old code ignored the flag entirely, used the
            // literal configured class, and assigned every column including 0.
            //
            // The stdClass fallback is implemented WITHOUT class_exists(): elephc's
            // class_exists() is an AOT constant-fold (src/codegen/lower_inst/builtins.rs:
            // lower_class_like_exists requires a CONST STRING operand) and simply does not
            // compile against a runtime string, which is the only kind of string that can
            // ever reach here. What IS available is the semantics of dynamic `new` itself:
            // `new $name()` lowers to DynamicObjectNewMixed, whose runtime miss path
            // (__rt_new_by_name, src/codegen_support/runtime/objects/new_by_name.rs) returns
            // PHP **null** for a name in no class table. So the construction attempt IS the
            // existence probe — one dynamic-new, no second lookup — and a null result is
            // precisely php-src's "zend_lookup_class() found nothing" arm.
            //
            // The two arms are kept as two RETURNS rather than one reassigned local: the
            // dynamic-new result is a Mixed, and rebinding that same local to a
            // concrete `new stdClass()` would ask the checker to unify Mixed with
            // Object(stdClass) in a slot that is about to be dynamic-property-written —
            // exactly the shape of the known untyped-dynamic-prop corruption. Two
            // straight-line returns give each object its own, single-typed local.
            if (($mode & 0x40000) != 0) {
                $_ctName = (string) $this->columnValue(0);
                return $this->assignColumnsFromOrStd(new $_ctName(), 1, $_count);
            }
            if ($this->fetchTarget !== null) {
                $_classTarget = $this->fetchTarget;
                return $this->assignColumns(new $_classTarget(), $_count);
            }
            // No target configured: php-src's own default for a bare FETCH_CLASS is
            // stdClass (pdo_stmt_setup_fetch_mode leaves stmt->fetch.cls.ce NULL, which
            // do_fetch resolves to zend_standard_class_def).
            return $this->assignColumns(new stdClass(), $_count);
        }
        if ($_base == 9) {
            if ($this->fetchTarget !== null) {
                return $this->assignColumns($this->fetchTarget, $_count);
            }
            // F-STMT-04: FETCH_INTO with NO object configured used to hand back a fresh,
            // anonymous stdClass — a silent success that threw the caller's row into an
            // object they never see. php-src raises HY000 "No fetch-into object specified."
            // (pdo_stmt.c:864-871, via pdo_raise_impl_error, hence errmode-aware: it
            // THROWS under ERRMODE_EXCEPTION and returns false under SILENT/WARNING).
            // FETCH_INTO without a target is not a mode, it is a mistake — the target is
            // the entire point of the mode.
            $this->failCode("HY000", "No fetch-into object specified.");
            return false;
        }
        if ($_base == 3) {
            $_numRow = [];
            for ($_i = 0; $_i < $_count; $_i++) {
                $_numRow[$_i] = $this->columnValue($_i);
            }
            return $_numRow;
        }
        if ($_base == 2) {
            $_assocRow = [];
            for ($_i = 0; $_i < $_count; $_i++) {
                $_name = $this->columnName($_i);
                $_assocRow[$_name] = $this->columnValue($_i);
            }
            return $_assocRow;
        }
        if ($_base == 11) {
            // P0-2 FETCH_NAMED: assoc-only, but when two or more result columns
            // share a name, group their values into a numerically-indexed array
            // under that one key instead of the last write silently winning
            // (verified against real PHP: `SELECT 1 a, 2 a` => ["a" => [1, 2]],
            // no numeric keys at all, and this grouping applies even when every
            // duplicate's value is NULL). A column name seen once still stores a
            // plain scalar, matching PHP exactly.
            //
            // Existence is tested by counting exact-name matches among the
            // already-visited columns rather than `array_key_exists()`/`isset()`:
            // the EIR backend does not support `array_key_exists()` on a Str key
            // ("unsupported EIR backend feature: array_key_exists key PHP type
            // Str", confirmed by compiling this branch), and `isset()` would
            // wrongly treat a NULL-valued first occurrence as "not yet seen"
            // (isset() is false for a key holding null), overwriting instead of
            // grouping it. Column counts are always small, so the O(n^2) scan
            // is cheap.
            $_names = [];
            for ($_i = 0; $_i < $_count; $_i++) {
                $_names[$_i] = $this->columnName($_i);
            }
            $_namedRow = [];
            for ($_i = 0; $_i < $_count; $_i++) {
                $_name = $_names[$_i];
                $_value = $this->columnValue($_i);
                $_priorCount = 0;
                for ($_j = 0; $_j < $_i; $_j++) {
                    if ($_names[$_j] === $_name) {
                        $_priorCount = $_priorCount + 1;
                    }
                }
                if ($_priorCount == 0) {
                    $_namedRow[$_name] = $_value;
                } elseif ($_priorCount == 1) {
                    $_namedRow[$_name] = [$_namedRow[$_name], $_value];
                } else {
                    $_existing = $_namedRow[$_name];
                    $_existing[] = $_value;
                    $_namedRow[$_name] = $_existing;
                }
            }
            return $_namedRow;
        }
        $_bothRow = [];
        for ($_i = 0; $_i < $_count; $_i++) {
            $_name = $this->columnName($_i);
            $_value = $this->columnValue($_i);
            $_bothRow[$_name] = $_value;
            $_bothRow[$_i] = $_value;
        }
        return $_bothRow;
    }

    public function fetchAll(int $mode = 0, mixed $classOrObject = null, mixed $ctorArgs = null): array {
        // $ctorArgs (FETCH_CLASS's constructor-argument array in PHP's
        // `fetchAll(PDO::FETCH_CLASS, 'Row', [...])` idiom) is accepted for
        // signature compatibility but not forwarded — the target class is always built
        // with no arguments, the same documented divergence as fetchObject()'s
        // $constructorArgs.
        //
        // NOTE that fetchAll() KEEPS its `mixed $classOrObject` second parameter while
        // fetch() (F-STMT-01) loses its own: that is not an inconsistency, it is php-src.
        // fetchAll's stub really does take the fetch-mode's extra arguments
        // (`fetchAll(int $mode = PDO::FETCH_DEFAULT, mixed ...$args)`); fetch's really
        // does not (its 2nd parameter is `int $cursorOrientation`). The two methods
        // diverge in php-src exactly as they now diverge here.
        $_unusedCtorArgs = $ctorArgs;
        if ($mode == 0) {
            $mode = $this->fetchMode;
        }
        $_base = $mode & 0xFFFF;
        // F-STMT-03: FETCH_LAZY is rejected HERE, and ONLY here. php-src's
        // pdo_stmt_verify_mode takes a `fetch_all` flag and refuses FETCH_LAZY on that
        // arm alone — fetchAll() is the one place real PHP forbids it, because a lazy
        // PDORow is a view onto the CURRENT row and a list of them would all alias the
        // last one. This prelude used to have the restriction exactly BACKWARDS: it
        // rejected LAZY in fetch() (where php-src allows it) and accepted it here (where
        // php-src does not). Message verbatim from php-src.
        if ($_base == 1) {
            throw new ValueError("PDOStatement::fetchAll(): Argument #1 (\$mode) cannot be PDO::FETCH_LAZY");
        }
        if ($_base == 10) {
            // P0-3 FETCH_FUNC: real PHP calls `$callback(...$columns)` once per
            // row and collects the returns. The callback would have to arrive
            // through this method's existing `mixed $classOrObject` slot (kept
            // Mixed so it can also carry FETCH_CLASS's class name / FETCH_INTO's
            // object), but elephc's checker refuses to invoke a Mixed value,
            // refuses to pass a Mixed value to any `callable`-typed parameter
            // (tried via a private helper and via call_user_func_array()), and
            // a `callable`-typed parameter cannot be given a default value
            // (tried null, a string, and a closure literal — all rejected at
            // the declaration itself), so it cannot be made optional on this
            // signature either. Every route was a dead end without a new bridge
            // extern (out of scope for this slice), so this fails loudly
            // instead of returning the silent BOTH-shaped garbage rows.
            throw new PDOException("PDO::FETCH_FUNC is not supported");
        }
        if ($_base == 12) {
            // FETCH_KEY_PAIR: aggregate the two-column result into [col0 => col1].
            // Stepped directly (not via fetch()) so the map is built exactly like
            // FETCH_ASSOC, avoiding an intermediate single-entry return array.
            if (!$this->executed) {
                return [];
            }
            $_pairs = [];
            while (true) {
                $_krc = $this->stepCursor();
                if ($_krc < 0) {
                    $this->fail(elephc_pdo_errmsg($this->conn));
                    break;
                }
                if ($_krc == 0) {
                    break;
                }
                if (elephc_pdo_column_count($this->stmt) != 2) {
                    // P2-b: errMode-aware, matching fetch()'s own KEY_PAIR check
                    // above (SILENT/WARNING break out and return whatever pairs
                    // were already collected instead of throwing).
                    $this->failCode("HY000", "PDO::FETCH_KEY_PAIR fetch mode requires the result set to contain exactly 2 columns.");
                    break;
                }
                $_kk = $this->columnValue(0);
                $_vv = $this->columnValue(1);
                $_pairs[$_kk] = $_vv;
            }
            return $_pairs;
        }
        // The 2nd argument is applied to the STATEMENT before the row loop, never handed
        // to fetch() — php-src does the same (PHP_METHOD(PDOStatement, fetchAll) writes
        // stmt->fetch.column / stmt->fetch.cls.ce up front and then loops do_fetch), and
        // since F-STMT-01 fetch() has no target parameter to hand it to anyway.
        if ($_base == 7) {
            // FETCH_COLUMN: `stmt->fetch.column = Z_LVAL(arg2)`. Without this,
            // fetchAll(PDO::FETCH_COLUMN, $n) would silently return column 0 regardless
            // of $n, since fetch()'s FETCH_COLUMN branch reads $this->fetchColumn.
            if ($classOrObject !== null) {
                $this->fetchColumn = (int) $classOrObject;
            } elseif (($mode & 0x10000) != 0) {
                // F-STMT-15: FETCH_COLUMN|FETCH_GROUP with NO explicit index defaults the
                // VALUE column to 1, not 0 — php-src's fetchAll() spells this out
                // (`stmt->fetch.column = arg2 ? … : (how & PDO_FETCH_GROUP ? 1 : 0)`),
                // and it is what makes the classic idiom work: on `SELECT type, name`,
                // `fetchAll(FETCH_GROUP|FETCH_COLUMN)` gives [type => [name, name, …]].
                // Column 0 is already spoken for as the grouping key, so defaulting the
                // value to it too would return [type => [type, type, …]].
                $this->fetchColumn = 1;
            } else {
                // php-src: `stmt->fetch.column = arg2 ? Z_LVAL(arg2) : (how &
                // PDO_FETCH_GROUP ? 1 : 0)` — the neither-branch of that ternary.
                // Without this, a plain `fetchAll(PDO::FETCH_COLUMN)` (no index, no
                // GROUP) would silently reuse whatever index a PRIOR
                // `fetchAll(FETCH_COLUMN, $n)` call left on $this->fetchColumn instead
                // of resetting to column 0.
                $this->fetchColumn = 0;
            }
        } elseif (($_base == 8 || $_base == 9) && $classOrObject !== null) {
            // FETCH_CLASS's class name / FETCH_INTO's object: `stmt->fetch.cls.ce`.
            $this->fetchTarget = $classOrObject;
        }
        // F-STMT-15: FETCH_GROUP (0x10000) and FETCH_UNIQUE (0x30000 — note it CONTAINS
        // the GROUP bit, so it is tested first) reshape the whole result set around a key
        // taken from column 0. They used to throw "not yet supported"; they are now real.
        if (($mode & 0x10000) != 0) {
            // Two combinations stay refused rather than faked, both because column 0 is
            // already consumed as the grouping key and something else wants it too:
            //  - FETCH_CLASSTYPE also reads column 0 (as the class name). php-src resolves
            //    the collision by consuming TWO columns (key from 0, class from 1, props
            //    from 2), which is a shape no caller of this prelude has ever been able to
            //    ask for, so it is refused rather than invented.
            //  - FETCH_BOUND/FETCH_INTO/FETCH_NAMED under GROUP have no meaningful
            //    per-group row here (BOUND writes to bound columns this prelude does not
            //    support, INTO would hand every group the SAME object, NAMED's duplicate-
            //    name grouping is a second, orthogonal reshaping). Loud beats silently
            //    wrong: a caller gets an error naming the combination, not a plausible
            //    array of the wrong shape.
            if (($mode & 0x40000) != 0) {
                throw new PDOException("PDO::FETCH_CLASSTYPE is not supported with PDO::FETCH_GROUP or PDO::FETCH_UNIQUE");
            }
            if ($_base != 2 && $_base != 3 && $_base != 4 && $_base != 5 && $_base != 7 && $_base != 8) {
                throw new PDOException("PDO::FETCH_GROUP and PDO::FETCH_UNIQUE are not supported with this fetch mode");
            }
            if (!$this->executed) {
                return [];
            }
            // Both modes CONSUME COLUMN 0 as the key — it becomes the array key and is
            // excluded from the row (groupRow() starts at column 1). They differ only in
            // what a key maps to:
            //   FETCH_GROUP  -> a LIST of every row that carried that key, in result order
            //                   (php-src: add_next_index_zval into the group's array);
            //   FETCH_UNIQUE -> ONE row, LAST WRITE WINS (php-src: zend_symtable_update, a
            //                   plain overwrite — it does not complain about a duplicate).
            //
            // FETCH_UNIQUE (0x30000) is a SUPERSET of FETCH_GROUP (0x10000), not a sibling
            // of it, so "is this unique?" must test the whole 0x30000 mask — a bare
            // `& 0x20000` would also accept a nonsense 0x20000-without-GROUP mode, and a
            // bare `& 0x10000` (the caller's own dispatch test above) is true for BOTH.
            //
            // The key is CAST TO STRING, exactly as php-src does (`convert_to_string`)
            // before it ever reaches the hash table. DIVERGENCE, and the only one here:
            // real PHP's array then folds an integer-LOOKING string key back to an int key,
            // so a grouping column holding 1 yields `$out[1]`; elephc's array keeps "1" a
            // string key, so read it back as `$out["1"]`. A non-numeric key (the
            // overwhelmingly common case: a type name, a status, a category) is identical.
            //
            // TWO TYPES OF THE SAME KEY are carried per row, on purpose. The split is what
            // makes this both COMPILE and not CRASH, and every op below is one this backend
            // is known to support:
            //
            //   $_gkeyM (groupKey(), `mixed`) keys the OUTPUT array. A statically Str-typed
            //     key would make $_out a genuine AssocArray, and returning THAT from this
            //     method's `: array` needs an AssocArray -> Array(Mixed) conversion the EIR
            //     backend does not implement. A Mixed key keeps $_out an Array(Mixed) — the
            //     shape FETCH_KEY_PAIR above already relies on, with columnValue()'s Mixed
            //     return as its key.
            //
            //   $_gkeyS (a plain `(string)`) keys the bucket map $_groups and the presence
            //     map $_present. A Str-keyed READ and STORE are both proven — FETCH_NAMED
            //     above does exactly that on $_namedRow.
            //
            // EXISTENCE IS TESTED BY A count() PROBE, not by isset()/array_key_exists(),
            // because NEITHER is available here:
            //   - isset() on a MIXED key COMPILES and then SIGSEGVs at run time (verified:
            //     dropping just that branch makes the same loop run clean, and FETCH_UNIQUE,
            //     whose store tests no membership, always passed);
            //   - isset() on a STR key does not compile at all ("runtime_call with receiver
            //     PHP type Void");
            //   - array_key_exists() on a Str key is unsupported too, as FETCH_NAMED's own
            //     comment above records.
            // Storing a key into $_present grows the map ONLY when that key is new, so the
            // count DELTA is an exact, allocation-free existence test that needs to read
            // nothing. It is sound for any key value, null included.
            //
            // FETCH_NAMED's alternative — counting prior matches by hand — is O(n^2), which
            // is fine across a row's COLUMNS but not here, where n is the number of ROWS.
            $_unique = ($mode & 0x30000) == 0x30000;
            $_present = [];
            $_groups = [];
            $_order = [];
            $_bn = 0;
            $_out = [];
            while (true) {
                $_grc = $this->stepCursor();
                if ($_grc < 0) {
                    $this->fail(elephc_pdo_errmsg($this->conn));
                    break;
                }
                if ($_grc == 0) {
                    break;
                }
                $_gcount = elephc_pdo_column_count($this->stmt);
                $_gkeyM = $this->groupKey(0);
                $_gkeyS = (string) $_gkeyM;
                $_grow = $this->groupRow($_base, $_gcount);
                if ($_unique) {
                    // LAST WRITE WINS (php-src: zend_symtable_update, a plain overwrite that
                    // neither detects nor complains about a duplicate key), so no membership
                    // test is needed at all — the store IS the semantics.
                    $_out[$_gkeyM] = $_grow;
                    continue;
                }
                $_before = count($_present);
                $_present[$_gkeyS] = 1;
                if (count($_present) > $_before) {
                    // First sighting of this key: open its bucket, and remember the key (in
                    // its Mixed form, for the output store) at its first-seen position.
                    $_groups[$_gkeyS] = [$_grow];
                    $_order[$_bn] = $_gkeyM;
                    $_bn = $_bn + 1;
                } else {
                    // Append to the existing bucket. Read / append / write back rather than
                    // `$_groups[$_gkeyS][] = $_grow`: a nested append through an element is a
                    // known EIR write-through gap, and FETCH_NAMED uses this same three-step
                    // form for the same reason.
                    $_bucket = $_groups[$_gkeyS];
                    $_bucket[] = $_grow;
                    $_groups[$_gkeyS] = $_bucket;
                }
            }
            if (!$_unique) {
                // Assembled in FIRST-SEEN order, which is php-src's: a group is created when
                // its key is first met and later rows are appended to it, so the groups come
                // out in the order their keys first appeared in the result set. Nothing is
                // ever read back out of $_out — it is written exactly once per distinct key.
                for ($_gi = 0; $_gi < $_bn; $_gi++) {
                    $_gkOut = $_order[$_gi];
                    $_out[$_gkOut] = $_groups[(string) $_gkOut];
                }
            }
            return $_out;
        }
        $_rows = [];
        while (true) {
            $_row = $this->fetch($mode);
            if ($_row === false) {
                break;
            }
            $_rows[] = $_row;
        }
        return $_rows;
    }

    // F-STMT-02: assignColumnsFrom(), plus php-src's "class not found -> stdClass" arm
    // (zend_lookup_class() failing, pdo_stmt.c:805-829). $object is whatever a dynamic
    // `new $name()` produced: the object, or NULL when the name is in no class table.
    //
    // The not-found probe is the CONSTRUCTION ITSELF — there is no class_exists() call and
    // cannot be one: elephc's class_exists() is an AOT constant-fold (lower_class_like_exists
    // needs a CONST string operand) and cannot see the runtime string that is the only kind
    // ever reaching here. __rt_new_by_name returns PHP null for an unknown name, which is
    // exactly php-src's not-found arm, so one dynamic-new answers both questions at once.
    //
    // The null test lives HERE, on a PARAMETER, rather than at the call site on a local, and
    // that placement is load-bearing: routing a dynamic-new's result through a caller LOCAL
    // MISCOMPILES — the object reaches the callee no longer an instance of its class and
    // with none of its properties. Written INLINE as the argument (`new $_ctName()` straight
    // into this call) it arrives sound, and a parameter then holds it safely. Verified both
    // ways; the local form silently produced a property-less non-instance.
    private function assignColumnsFromOrStd(mixed $object, int $start, int $count): mixed {
        if ($object === null) {
            return $this->assignColumnsFrom(new stdClass(), $start, $count);
        }
        return $this->assignColumnsFrom($object, $start, $count);
    }

    // F-STMT-15: the FETCH_GROUP / FETCH_UNIQUE grouping key, taken from column 0 and CAST
    // TO STRING exactly as php-src does (pdo_stmt.c do_fetch: `convert_to_string(&grp_val)`
    // before the key ever reaches the hash table).
    //
    // Declared `: mixed` DELIBERATELY, not `: string` — see the call site. Returning the
    // key as Mixed is what keeps fetchAll()'s $_out an Array(Mixed) instead of promoting it
    // to a statically-typed AssocArray it could then not return through `: array`.
    private function groupKey(int $index): mixed {
        return (string) $this->columnValue($index);
    }

    // F-STMT-15: builds ONE grouped row — the part of the result that is NOT the key —
    // in the shape the base fetch mode asks for, always starting from COLUMN 1 because
    // column 0 was consumed as the grouping key by the caller.
    //
    // The numeric keys of FETCH_NUM/FETCH_BOTH are RE-INDEXED FROM 0, not left as the
    // original column positions: php-src walks the row with two cursors — the column
    // index `i` (which starts at 1 after the key was taken) and the output index `idx`
    // (which starts at 0) — so the first column AFTER the key lands at [0]. A row that
    // kept its original offsets would start at [1] and have no [0] at all.
    private function groupRow(int $base, int $count): mixed {
        if ($base == 7) {
            // FETCH_COLUMN: the single configured value column (defaulted to 1 by
            // fetchAll() when GROUP is set — see there), not a row at all.
            return $this->columnValue($this->fetchColumn);
        }
        if ($base == 5) {
            return $this->assignColumnsFrom(new stdClass(), 1, $count);
        }
        if ($base == 8) {
            if ($this->fetchTarget !== null) {
                $_gClass = $this->fetchTarget;
                return $this->assignColumnsFrom(new $_gClass(), 1, $count);
            }
            return $this->assignColumnsFrom(new stdClass(), 1, $count);
        }
        if ($base == 3) {
            $_gNum = [];
            $_gIdx = 0;
            for ($_i = 1; $_i < $count; $_i++) {
                $_gNum[$_gIdx] = $this->columnValue($_i);
                $_gIdx = $_gIdx + 1;
            }
            return $_gNum;
        }
        if ($base == 2) {
            $_gAssoc = [];
            for ($_i = 1; $_i < $count; $_i++) {
                $_gName = $this->columnName($_i);
                $_gAssoc[$_gName] = $this->columnValue($_i);
            }
            return $_gAssoc;
        }
        // FETCH_BOTH (4), the remaining accepted base — fetchAll()'s own guard has
        // already rejected every mode that is not one of the six handled here.
        $_gBoth = [];
        $_gPos = 0;
        for ($_i = 1; $_i < $count; $_i++) {
            $_gBothName = $this->columnName($_i);
            $_gBothVal = $this->columnValue($_i);
            $_gBoth[$_gBothName] = $_gBothVal;
            $_gBoth[$_gPos] = $_gBothVal;
            $_gPos = $_gPos + 1;
        }
        return $_gBoth;
    }

    public function fetchColumn(int $column = 0): mixed {
        if (!$this->executed) {
            return false;
        }
        $_rc = $this->stepCursor();
        if ($_rc < 0) {
            $this->fail(elephc_pdo_errmsg($this->conn));
            return false;
        }
        if ($_rc == 0) {
            return false;
        }
        // P2-11: bounds-check against the row actually fetched (verified against
        // real PHP: an out-of-range index on an EMPTY result set just returns
        // `false` like any other no-more-rows call — the ValueError only fires
        // once a row exists to check the index against).
        if ($column < 0) {
            throw new ValueError("Column index must be greater than or equal to 0");
        }
        if ($column >= $this->columnCount()) {
            throw new ValueError("Invalid column index");
        }
        return $this->columnValue($column);
    }

    public function closeCursor(): bool {
        // Free the result set and require a re-execute before the next fetch,
        // matching PHP: after closeCursor() a fetch on the forward-only cursor
        // returns false until execute() runs again.
        elephc_pdo_reset($this->stmt);
        $this->executed = false;
        // Defensive: a pending pre-step (see execute()'s comment) would
        // otherwise reference a row that this reset just discarded.
        // Practically unreachable today (fetch()'s `!executed` guard already
        // blocks stepCursor() from running until the next execute() call
        // overwrites it), but keeping the flag in lockstep with `executed`
        // avoids relying on that as an invariant here.
        $this->hasPendingStep = false;
        return true;
    }

    public function fetchObject(?string $class = "stdClass", array $constructorArgs = []): mixed {
        // Constructor args are accepted for signature compatibility but not
        // forwarded (the object is built with no arguments) — a documented
        // divergence from PHP, which passes them to the class constructor.
        $_unusedArgs = $constructorArgs;
        if (!$this->executed) {
            return false;
        }
        $_rc = $this->stepCursor();
        if ($_rc < 0) {
            $this->fail(elephc_pdo_errmsg($this->conn));
            return false;
        }
        if ($_rc == 0) {
            return false;
        }
        $_count = elephc_pdo_column_count($this->stmt);
        if ($class === null || $class === "stdClass") {
            return $this->assignColumns(new stdClass(), $_count);
        }
        return $this->assignColumns(new $class(), $_count);
    }

    public function rowCount(): int {
        // The affected-row count captured at execute() time. Reliable for DML
        // (INSERT/UPDATE/DELETE); for SELECT it is driver-dependent, exactly as
        // in PHP. Snapshotting keeps it stable against later statements sharing
        // the connection.
        return $this->rowCount;
    }

    public function columnCount(): int {
        return elephc_pdo_column_count($this->stmt);
    }

    public function getAttribute(int $name): mixed {
        // P2-16: Pdo\Sqlite::ATTR_READONLY_STATEMENT is a LIVE sqlite3_stmt_readonly()
        // read rather than a stored value — it reflects the actual prepared
        // statement, not a value the caller set. The bridge reports 0 for a
        // non-SQLite statement, which reads back as false there too.
        if ($name == 1001) {
            return elephc_pdo_stmt_readonly($this->stmt) === 1;
        }
        // P1-i: ATTR_EMULATE_PREPARES answers from the prepare()-time snapshot of
        // the owning connection's stored value (see setEmulatePrepares()); real
        // PHP answers this one from a live driver flag (`generic_stmt_attr_get`),
        // but none of elephc's drivers ever emulates a prepare, so the
        // snapshot is the closest honest analogue.
        if ($name == 20) {
            return $this->emulatePrepares;
        }
        // P1-i/P3: no driver in this bridge registers a statement attribute
        // hook, so every other attribute mirrors php-src's IM001 "This driver
        // doesn't support getting attributes" (pdo_raise_impl_error) —
        // errMode-aware: EXCEPTION throws, WARNING/SILENT fall through and
        // return `false` (verified against php-src's
        // `PHP_METHOD(PDOStatement, getAttribute)`: the no-hook branch is
        // `RETURN_FALSE`, not NULL).
        $this->failCode("IM001", "This driver doesn't support getting attributes");
        return false;
    }

    public function setAttribute(int $attribute, mixed $value): bool {
        // P1-i: no driver in this bridge registers a statement attribute hook, so
        // every attribute mirrors php-src's IM001 "This driver doesn't support
        // setting attributes" (pdo_raise_impl_error) instead of the previous
        // unconditional accept-and-store. errMode-aware, like every other
        // statement failure; always returns false regardless of mode.
        //
        // BOTH parameters are explicitly parked: no attribute is supported, so neither the
        // name nor the value is ever read, and an unparked one is a compiler warning emitted
        // against every program that so much as links this prelude.
        $_unusedAttribute = $attribute;
        $_unusedValue = $value;
        $this->failCode("IM001", "This driver doesn't support setting attributes");
        return false;
    }

    public function nextRowset(): bool {
        // P2-c/P3: SQLite and PostgreSQL genuinely have no further-rowset concept
        // here (pdo_sqlite/pdo_pgsql each materialize exactly one result set per
        // prepared statement), so mirror php-src's IM001 "driver does not
        // support multiple rowsets" (pdo_raise_impl_error, exact wording
        // verified against php-src) instead of silently returning false —
        // errMode-aware like every other statement failure.
        //
        // MySQL is the one driver that genuinely supports multiple rowsets over
        // the wire (a CALL returning several result sets, or a multi-statement
        // query), but this bridge's mysql client only ever materializes the
        // first one per prepared statement (see docs/php/pdo.md's Limitations
        // section) — there is no real second rowset to raise IM001 about or to
        // advance to, so it still returns false, just without the error: this
        // is a "no more rowsets" answer, not a "driver can't do this" one.
        if (elephc_pdo_driver_name($this->conn) === "mysql") {
            return false;
        }
        $this->failCode("IM001", "driver does not support multiple rowsets");
        return false;
    }

    public function getColumnMeta(int $column): array|bool {
        // Reduced PDOStatement::getColumnMeta: the column name plus the PDO and
        // native type derived from the bridge's per-column type code (1=INTEGER,
        // 2=FLOAT, 3=TEXT, 4=BLOB, 5=NULL) — always the runtime STORAGE-CLASS name
        // (P1-8), matching PHP's ext/pdo_sqlite/sqlite_statement.c exactly (verified
        // against a real PHP 8.5 CLI): a BLOB column reports native_type "string"
        // with "blob" pushed into flags and pdo_type PARAM_STR, never its own
        // native_type/pdo_type. For SQLite, PHP's remaining metadata (len, precision,
        // table) genuinely has no source — pdo_sqlite does not emit those keys at all —
        // so they are present with neutral values rather than omitted, and callers that
        // read them do not error. Returns false for an out-of-range column index.
        //
        // P2-h: also false when the statement hasn't been executed yet — there is
        // no result set (or, for a non-SELECT statement, no columns) to describe.
        //
        // P2-k / F-PG-01 / F-PG-02: a `pgsql:` statement instead reports PostgreSQL's real
        // per-column metadata, in FULL as of v26. `elephc_pdo_column_type_oid` returns the
        // column's `PQftype` OID (0 for a non-pg statement or out-of-range index),
        // threaded from the prepared statement's retained `postgres::types::Type`; a
        // non-zero OID selects the pg branch below, which reports the server's native_type
        // (`int4`/`bool`/`bytea`/… via `elephc_pdo_column_native_type`, i.e.
        // `pg_type.typname`), the matching `pdo_type` (BOOL→PARAM_BOOL,
        // {INT2,INT4,INT8}→PARAM_INT, {BYTEA,OID}→PARAM_LOB, else PARAM_STR — the
        // exact switch in php-src's ext/pdo_pgsql/pgsql_statement.c), the `pgsql:oid` key,
        // and now `len` (PQfsize), `precision` (PQfmod) and `pgsql:table_oid` (PQftable).
        // php-src's `table` NAME still needs a `pg_class` catalog lookup this bridge does
        // not perform, so `table` alone stays empty (present, not omitted, so a caller
        // reading it never errors) — it is the one pg key left neutral.
        //
        // F-MY-08: a `mysql:` statement gets OID 0 and falls through to the generic branch
        // like SQLite, but its native_type is then OVERRIDDEN with MySQL's own wire-type
        // name ("LONG", "VAR_STRING", "NEWDECIMAL", …) — see that branch. SQLite falls
        // through unchanged, storage class and all.
        //
        // P3: a negative column index throws a `ValueError` BEFORE the
        // executed/range checks below, mirroring php-src's exact ordering and
        // message wording (verified against php-src's
        // `PHP_METHOD(PDOStatement, getColumnMeta)`: `zend_argument_value_error`
        // fires from parameter validation, ahead of any driver dispatch or
        // executed-state check) — only a column index `>=` the real column
        // count still returns `false` (php-src only RETURN_FALSEs for that
        // case, never for a negative one).
        if ($column < 0) {
            throw new ValueError("PDOStatement::getColumnMeta(): Argument #1 (\$column) must be greater than or equal to 0");
        }
        if (!$this->executed) {
            return false;
        }
        if ($column >= elephc_pdo_column_count($this->stmt)) {
            return false;
        }
        $_oid = elephc_pdo_column_type_oid($this->stmt, $column);
        if ($_oid > 0) {
            // pgsql (P2-k): describe with the real PostgreSQL type. pdo_type
            // mirrors php-src pdo_pgsql's OID switch exactly
            // (ext/pdo_pgsql/pgsql_statement.c:690-706) — BOOLOID (16) is
            // PARAM_BOOL (5); the integer family INT8/INT2/INT4 (20/21/23) is
            // PARAM_INT (1); OIDOID (26) shares the PARAM_LOB (3) case with
            // BYTEAOID (17) — `case OIDOID: case BYTEAOID:` is a literal pair in
            // that switch, because an OID is a large-object handle to pdo_pgsql,
            // not an integer value (F-PG-04: it was grouped with the ints here);
            // and every other OID (text/varchar/numeric/timestamptz/json/…) is
            // PARAM_STR (2). Raw integer literals here (not the PDO::PARAM_*
            // constants) match the storage-class branch below.
            $_pgType = 2;
            if ($_oid == 16) {
                $_pgType = 5;
            } elseif ($_oid == 17 || $_oid == 26) {
                $_pgType = 3;
            } elseif ($_oid == 20 || $_oid == 21 || $_oid == 23) {
                $_pgType = 1;
            }
            // F-PG-01/F-PG-02 (v26): the three remaining pg metadata fields, which used to
            // be hardcoded 0 / omitted.
            //
            // `pgsql:table_oid` is emitted UNCONDITIONALLY, **0 included** — php-src's
            // pgsql_stmt_get_column_meta adds the key on every column with no test at all,
            // and 0 is InvalidOid, the server's OWN answer for a column that is not a plain
            // table column (an expression, a literal, an aggregate). Suppressing the key on
            // 0 would make `array_key_exists('pgsql:table_oid', $meta)` diverge from real
            // PDO on exactly the columns where a caller is most likely to test it.
            //
            // `len` and `precision` are PQfsize() and PQfmod() STRAIGHT, and they are NOT
            // what the names suggest:
            //   * len is the TYPE's byte width when it has a fixed one (int4 -> 4,
            //     timestamp -> 8, uuid -> 16) and **-1** for any VARLENA — text, varchar,
            //     numeric, bytea, json, every array type. A VARCHAR(20) reports len -1,
            //     NOT 20.
            //   * precision is the RAW atttypmod, undecoded. VARCHAR(20)'s declared 20
            //     surfaces HERE, as 24 (20 + VARHDRSZ); NUMERIC(10,2) is 655366
            //     (((10 << 16) | 2) + 4).
            // Both are counter-intuitive and both are exactly what real PDO reports.
            // Decoding atttypmod into a human-readable precision here would be a
            // divergence dressed up as a courtesy — a caller who wants the real precision
            // must decode the modifier, precisely as it would have to against real PDO.
            return [
                "name" => $this->columnName($column),
                "native_type" => elephc_pdo_column_native_type($this->stmt, $column),
                "pdo_type" => $_pgType,
                "len" => elephc_pdo_column_len($this->stmt, $column),
                "precision" => elephc_pdo_column_precision($this->stmt, $column),
                "flags" => [],
                "table" => "",
                "pgsql:oid" => $_oid,
                "pgsql:table_oid" => elephc_pdo_column_table_oid($this->stmt, $column),
            ];
        }
        $_type = elephc_pdo_column_type($this->stmt, $column);
        $_native = "null";
        $_pdoType = 0;
        $_flags = [];
        if ($_type == 1) {
            $_native = "integer";
            $_pdoType = 1;
        } elseif ($_type == 2) {
            $_native = "double";
            $_pdoType = 2;
        } elseif ($_type == 3) {
            $_native = "string";
            $_pdoType = 2;
        } elseif ($_type == 4) {
            $_native = "string";
            $_pdoType = 2;
            $_flags[] = "blob";
        }
        // F-MY-08 (v26): a MySQL column reports MySQL's OWN wire-type name, not the
        // storage-class name derived just above. php-src's pdo_mysql builds native_type
        // from `type_to_name_native()`, whose PDO_MYSQL_NATIVE_TYPE_NAME macro stringifies
        // the MYSQL_TYPE_ suffix — so an INT column is "LONG", a VARCHAR is "VAR_STRING",
        // a DECIMAL is "NEWDECIMAL", a BLOB/TEXT is "BLOB". Those spellings are the whole
        // point: a caller inspecting native_type wants to know it has a NEWDECIMAL (which
        // MySQL hands over as a string to preserve exactness), not that the value
        // currently in the cell happens to look like a "string".
        //
        // The bridge returns "" for SQLite BY DESIGN, which is what keeps this branch a
        // no-op there: php-src's own sqlite driver reports the runtime STORAGE CLASS
        // ("integer"/"double"/"string"/"null") exactly as derived above, so SQLite's
        // output must stay byte-identical — and does. An empty string also covers an
        // unknown handle, an out-of-range index, and a MySQL wire type php-src's switch
        // has no case for (its `default:` OMITS the key entirely, so falling back to the
        // storage class is strictly more informative than php-src there, not less).
        //
        // pdo_type, len, precision and flags stay as derived: this widens the type NAME
        // only. MySQL's own PDO param-type mapping and column widths would need a
        // separate metadata channel, and no finding asks for them.
        $_myNative = elephc_pdo_column_native_type($this->stmt, $column);
        if ($_myNative !== "") {
            $_native = $_myNative;
        }
        $_meta = [
            "name" => $this->columnName($column),
            "native_type" => $_native,
            "pdo_type" => $_pdoType,
            "len" => 0,
            "precision" => 0,
            "flags" => $_flags,
            "table" => "",
        ];
        // P1-8: the column's DECLARED type (sqlite3_column_decltype) is a SEPARATE
        // "sqlite:decl_type" key — it must never overwrite native_type above. Empty
        // for an expression column with no declared type (or a non-SQLite driver,
        // where the bridge always reports an empty decltype), matching PHP's
        // omitting the key entirely in that case.
        $_decltype = elephc_pdo_column_decltype($this->stmt, $column);
        if ($_decltype !== "") {
            $_meta["sqlite:decl_type"] = $_decltype;
        }
        return $_meta;
    }

    public function debugDumpParams(): ?bool {
        // F-STMT-12: full php-src line shapes (pdo_stmt.c:1963-2020) — the SQL line, the
        // parameter count, then ONE block per bound parameter:
        //
        //     SQL: [<bytes>] <sql>
        //     Params:  <n>
        //     Key: Name: [<bytes>] :name      (named)   /   Key: Position #<paramno>:
        //     paramno=<paramno>
        //     name=[<bytes>] ":name"
        //     is_param=1
        //     param_type=<int>
        //
        // Note the two spaces after "Params:" and the QUOTED name on the `name=` line —
        // both are php-src's own spacing/quoting (`"paramno=" ZEND_LONG_FMT "\nname=[%zd]
        // \"%.*s\"\nis_param=%d\nparam_type=%d\n"`), not a typo here.
        //
        // "Sent SQL:" is correctly ABSENT: php-src prints it only when
        // stmt->active_query_string differs from the original, i.e. only for an EMULATED
        // prepare. elephc never emulates (every statement is a real driver prepare), so
        // there is no second string to show — the same reason php's own native-prepare
        // drivers omit the line.
        //
        // TWO documented divergences, both forced by the shape of the recorded binds
        // (param_type is NOT one of them — see $boundPhpTypes, which reproduces php-src's
        // reported type exactly on both bind paths):
        //   1. php-src's stmt->bound_params is a HASH keyed by name (named) or by paramno
        //      (positional), so re-binding the same parameter REPLACES its entry and
        //      `Params:` counts DISTINCT parameters. elephc records an append-only list, so
        //      `bindValue(1,'a'); bindValue(1,'b');` prints TWO blocks and `Params:  2`
        //      where php prints one and `Params:  1`. The value actually sent is the same
        //      (execute() replays in order, last write wins).
        //   2. paramno for a NAMED parameter: php-src stores -1 until the driver's
        //      EVT_NORMALIZE/EXEC_PRE hook resolves it, so a dump taken BEFORE the first
        //      execute() shows -1 and one taken after shows the 0-based slot (verified
        //      against real PHP). elephc resolves the name eagerly at bind time
        //      (bindValue()), so the resolved 0-based slot is shown from the start — the
        //      two agree from the first execute() onward, and -1 appears only when the
        //      placeholder does not exist in the SQL at all (which execute() then rejects
        //      with HY093). Faking the -1 would mean keying the dump off "has execute() run
        //      yet", which closeCursor() would then get wrong; the resolved slot is both
        //      honest and strictly more informative.
        echo "SQL: [" . strlen($this->queryString) . "] " . $this->queryString . "\n";
        $_pcount = count($this->boundValues);
        echo "Params:  " . $_pcount . "\n";
        for ($_i = 0; $_i < $_pcount; $_i++) {
            $_dname = (string) $this->boundNames[$_i];
            // php's paramno is 0-based; the recorded slot is the driver's 1-based index.
            $_dno = ((int) $this->boundParams[$_i]) - 1;
            $_dtype = (int) $this->boundPhpTypes[$_i];
            $_dlen = strlen($_dname);
            if ($_dname === "") {
                echo "Key: Position #" . $_dno . ":\n";
            } else {
                echo "Key: Name: [" . $_dlen . "] " . $_dname . "\n";
            }
            echo "paramno=" . $_dno . "\n";
            echo "name=[" . $_dlen . "] \"" . $_dname . "\"\n";
            // is_param is 1 for every entry of bound_params; php's 0 case is a bound COLUMN
            // (bindColumn), which lives in a different hash and is not dumped here.
            echo "is_param=1\n";
            echo "param_type=" . $_dtype . "\n";
        }
        // Always null (never false): php returns false only when it cannot open
        // php://output, which has no elephc equivalent.
        return null;
    }

    // Iterator: `foreach ($stmt as $key => $row)` walks the result set forward
    // using the statement's current fetch mode, with sequential integer keys —
    // matching PHP's PDOStatement Traversable behavior. The cursor is
    // forward-only, so rewind() only fetches the first row (it cannot seek back
    // to an already-consumed row).
    public function rewind(): void {
        $this->iterRow = $this->fetch($this->fetchMode);
        $this->iterKey = 0;
    }

    public function valid(): bool {
        return $this->iterRow !== false;
    }

    public function current(): mixed {
        return $this->iterRow;
    }

    public function key(): mixed {
        return $this->iterKey;
    }

    public function next(): void {
        $this->iterRow = $this->fetch($this->fetchMode);
        $this->iterKey = $this->iterKey + 1;
    }

    public function getIterator(): \Iterator {
        // PHP 8 declares `PDOStatement implements IteratorAggregate`, so
        // `getIterator()` is the documented way to obtain the traversable; this
        // prelude implements `Iterator` directly instead (see the F-STMT-11 note on the
        // class declaration for why that is deliberate), so the statement itself already
        // satisfies that contract and can hand back `$this` — `foreach ($stmt->getIterator()
        // as $row)` therefore walks the same forward-only cursor as `foreach ($stmt as $row)`,
        // exactly as it does in PHP.
        return $this;
    }

    public function __destruct() {
        // Finalize the prepared statement when the PDOStatement is collected. The
        // bridge ignores an unknown/already-finalized handle, so this is safe even
        // when the owning PDO connection was closed first (its close() already
        // finalized this statement).
        elephc_pdo_finalize($this->stmt);
    }

    // P2-17: mirrors \PDO::__clone() — PHP marks PDOStatement uncloneable too. A
    // shallow clone would produce a second owner of `$this->stmt`; whichever copy is
    // destructed first finalizes the handle out from under the survivor.
    public function __clone(): void {
        throw new Error("Trying to clone an uncloneable object of class " . get_class($this));
    }

    // F-CORE-15: mirrors \PDO::__serialize()/__sleep() (see the long rationale there) —
    // php-src marks PDOStatement `/** @not-serializable */` too, and elephc's
    // property-walking serialize() would otherwise emit this object's private `$stmt`
    // and `$conn` bridge handles into the blob, yielding a zombie statement on
    // unserialize(). Same php-src message shape, same plain `Exception` class, same
    // get_class($this) so the reported name is the object's real class.
    public function __serialize(): array {
        throw new Exception("Serialization of '" . get_class($this) . "' is not allowed");
    }

    public function __sleep(): array {
        throw new Exception("Serialization of '" . get_class($this) . "' is not allowed");
    }
}

// PHP 8.4 driver-specific PDO subclasses. They are returned by the DSN-dispatching
// `PDO::connect()` factory (defined above) and can also be constructed directly;
// each inherits the full base PDO connection surface (constructor, exec/query/
// prepare, transactions, quoting) from \PDO, and adds its driver-specific
// constants plus the driver methods that need no C->PHP callback. Still deferred:
// the callback methods (Pdo\Sqlite::createFunction / createAggregate /
// createCollation, Pdo\Pgsql::setNoticeCallback), which require a PHP callable to
// be invoked from C mid-query — elephc's FFI cannot yet marshal a callable to a C
// function pointer — and the connection-backed methods that need new bridge externs
// (getWarningCount, getPid, lob*/copy*, loadExtension, openBlob).
//
// The classes are declared in a BLOCK-form namespace: a statement-form
// `namespace Pdo;` would apply to every statement that follows it, and because
// this prelude is prepended ahead of user code that would silently re-namespace
// the entire user program. The block keeps the `Pdo\` scope contained, leaving
// the appended user code in the global namespace. `extends \PDO` is
// fully-qualified so it binds to the global prelude PDO regardless of scope.
// Builtins called from a method body here are `\`-qualified because an unqualified
// call inside the `Pdo` namespace does not fall back to the global function on
// every name-resolution path.
namespace Pdo {
    class Sqlite extends \PDO {
        // SQLite driver-specific constants (ext/pdo_sqlite). ATTR_* start at
        // PDO_ATTR_DRIVER_SPECIFIC (1000); OPEN_* mirror the SQLite C open flags;
        // DETERMINISTIC is the SQLITE_DETERMINISTIC function flag.
        const DETERMINISTIC = 2048;
        const OPEN_READONLY = 1;
        const OPEN_READWRITE = 2;
        const OPEN_CREATE = 4;
        const ATTR_OPEN_FLAGS = 1000;
        const ATTR_READONLY_STATEMENT = 1001;
        const ATTR_EXTENDED_RESULT_CODES = 1002;
        // 8.5-READINESS: `Pdo\Sqlite::ATTR_BUSY_STATEMENT`, `ATTR_EXPLAIN_STATEMENT`,
        // `ATTR_TRANSACTION_MODE`, `TRANSACTION_MODE_*`, `EXPLAIN_MODE_*`, and the
        // authorizer-callback return codes `OK`/`DENY`/`IGNORE` are deliberately
        // excluded here — php-src only adds them in PHP 8.5 (alongside
        // `Pdo\Sqlite::setAuthorizer()`, itself unsupported — see docs/php/pdo.md).
        // Add them when elephc's PHP target moves to 8.5, mirroring the FETCH_*
        // 8.5-readiness note on test_pdo_constants_present.

        // Roots the collation / user-function callbacks registered on this
        // connection. SQLite keeps a raw C pointer to each callback's compiled-PHP
        // descriptor for the connection's lifetime, so the descriptor must stay
        // reachable from PHP; this array is that GC root.
        private array $udfCallbacks;

        public function __construct(string $dsn, ?string $username = null, ?string $password = null, ?array $options = null) {
            // F-CORE-01/F-CORE-11: resolve an indirect `uri:` DSN FIRST (php-src resolves
            // it before it compares the DSN's driver against the called scope), then reject
            // a DSN belonging to another driver BEFORE any connection is attempted. The
            // resolved DSN is what goes up to \PDO, so the file is read exactly once —
            // resolveDsnUri() is a no-op on an already-resolved DSN.
            $_sqliteDsn = $this->resolveDsnUri($dsn);
            $this->checkDriverSubclassDsn($_sqliteDsn, "Pdo\\Sqlite", "sqlite");
            // Forward to \PDO to open the connection, then initialise the callback
            // root (an uninitialised typed array property is not implicitly []).
            parent::__construct($_sqliteDsn, $username, $password, $options);
            $this->udfCallbacks = [];
        }

        public function loadExtension(string $name): void {
            // Loads a SQLite extension library by path (its entry point is
            // auto-derived, as PHP's loadExtension does), throwing on failure.
            // Extension loading runs native code from the named library, so it
            // weakens the standalone-binary guarantee — use only trusted extensions.
            //
            // F-SQLT-05: an EMPTY name is rejected during argument validation, ahead of
            // any driver dispatch — php-src's pdo_sqlite.c:80-87 is
            // `if (ZSTR_LEN(extension) == 0) { zend_argument_must_not_be_empty_error(1);
            // RETURN_THROWS(); }`, whose ValueError reads "…(): Argument #1 ($name) must
            // not be empty". elephc used to hand "" straight to sqlite3_load_extension and
            // surface its failure as the generic PDOException below, which is both the
            // wrong exception class and the wrong stage.
            if ($name === "") {
                throw new \ValueError("Pdo\\Sqlite::loadExtension(): Argument #1 (\$name) must not be empty");
            }
            if (\elephc_pdo_load_extension($this->connectionId(), $name) !== 1) {
                throw new \PDOException("Failed to load SQLite extension: " . $name);
            }
        }

        public function openBlob(string $table, string $column, int $rowid, ?string $dbname = "main", int $flags = 1): mixed {
            // Opens a BLOB cell as a stream resource. Divergence from PHP: this is a
            // read-whole snapshot — the whole BLOB is read into an in-memory stream, so
            // reads (fread/stream_get_contents) work fully but writes are not flushed
            // back to the row. $flags defaults to 1 (self::OPEN_READONLY) written as a
            // literal, since a constant of the class being defined does not resolve as
            // an int default here, and is accepted only for signature compatibility (a
            // read-write handle is not honored). Returns false if the row/column cannot
            // be opened (matching PHP's failure return).
            $_unused = $flags;
            $_db = ($dbname === null) ? "main" : $dbname;
            $_len = \elephc_pdo_blob_read($this->connectionId(), $table, $column, $rowid, $_db);
            return $this->blobStream($_len);
        }

        public function createCollation(string $name, callable $callback): bool {
            // Registers a custom collation `$name` backed by a compiled-PHP
            // comparator `$callback($a, $b): int` (returning <0, 0, >0). The callable
            // is decomposed here into its descriptor pointer and the shared codegen
            // collation adapter address, so the bridge extern receives two plain
            // `ptr` args and never a `callable`. The callback is rooted in
            // $this->udfCallbacks first because SQLite keeps a C pointer to its
            // descriptor for the connection's lifetime. The key is namespaced so a
            // same-named collation and scalar function do not evict each other's GC
            // root. Only closures and first-class callables are supported (their value
            // is a descriptor pointer); a string or array callable is rejected at
            // compile time by __elephc_callable_ptr.
            $this->udfCallbacks["collation:" . $name] = $callback;
            $_descriptor = \__elephc_callable_ptr($callback);
            $_adapter = \__elephc_pdo_adapter_addr(0);
            return \elephc_pdo_create_collation($this->connectionId(), $name, $_descriptor, $_adapter) === 1;
        }

        public function createFunction(string $function_name, callable $callback, int $num_args = -1, int $flags = 0): bool {
            // Registers a scalar SQL function `$function_name` backed by a compiled-PHP
            // `$callback(...$args): mixed` invoked once per row. Like createCollation,
            // the callable is decomposed here into its descriptor pointer and the shared
            // codegen scalar adapter address, so the bridge extern receives two plain
            // `ptr` args and never a `callable`. The callback is rooted in
            // $this->udfCallbacks (under a function-namespaced key) first because SQLite
            // keeps a C pointer to its descriptor for the connection's lifetime.
            // $num_args is the declared arity (-1 = variadic); $flags carries
            // self::DETERMINISTIC. Only closures and first-class callables are supported;
            // a string or array callable is rejected at compile time by
            // __elephc_callable_ptr. Parameter names match the PHP stub
            // (`createFunction(string $function_name, callable $callback, int $num_args = -1, int $flags = 0)`)
            // so named-argument calls resolve; the extern call below uses positions,
            // so the rename is otherwise behavior-neutral.
            $this->udfCallbacks["function:" . $function_name] = $callback;
            $_descriptor = \__elephc_callable_ptr($callback);
            $_adapter = \__elephc_pdo_adapter_addr(1);
            return \elephc_pdo_create_function($this->connectionId(), $function_name, $num_args, $flags, $_descriptor, $_adapter) === 1;
        }

        public function createAggregate(string $name, callable $step, callable $finalize, int $numArgs = -1): bool {
            // Registers an aggregate SQL function `$name` backed by a compiled-PHP
            // step + finalize pair: `$step($context, $rownumber, ...$values): mixed`
            // runs once per row (returning the new accumulator, null-seeded on the
            // first row) and `$finalize($context, $rownumber): mixed` produces the
            // group result. Each callable is decomposed into its descriptor pointer
            // and the shared codegen adapter address (kinds 2 and 3), so the bridge
            // extern receives four plain `ptr` args and never a `callable`. Both
            // callables are rooted in $this->udfCallbacks (under distinct keys so
            // neither evicts the other's GC root) because SQLite keeps a C pointer to
            // each descriptor for the connection's lifetime. Only closures and
            // first-class callables are supported; a string or array callable is
            // rejected at compile time by __elephc_callable_ptr.
            $this->udfCallbacks["aggregate_step:" . $name] = $step;
            $this->udfCallbacks["aggregate_final:" . $name] = $finalize;
            $_stepDesc = \__elephc_callable_ptr($step);
            $_stepAdapter = \__elephc_pdo_adapter_addr(2);
            $_finalDesc = \__elephc_callable_ptr($finalize);
            $_finalAdapter = \__elephc_pdo_adapter_addr(3);
            return \elephc_pdo_create_aggregate($this->connectionId(), $name, $numArgs, $_stepDesc, $_stepAdapter, $_finalDesc, $_finalAdapter) === 1;
        }
    }

    class Mysql extends \PDO {
        // MySQL/MariaDB driver-specific attribute constants (ext/pdo_mysql, mysqlnd
        // build — the PHP default). Values start at PDO_ATTR_DRIVER_SPECIFIC (1000).
        // The libmysqlclient-only ATTR_MAX_BUFFER_SIZE / ATTR_READ_DEFAULT_* are
        // intentionally omitted (absent under mysqlnd, and their presence would shift
        // every value from ATTR_COMPRESS upward).
        const ATTR_USE_BUFFERED_QUERY = 1000;
        const ATTR_LOCAL_INFILE = 1001;
        // P1-9 (minimal wiring): honored by PDO::__construct's constructor-options
        // loop, which threads the raw SQL string through to the bridge's connect
        // path (my.rs::MyConn::open -> OptsBuilder::init). Every other
        // driver-specific ATTR_* below remains inert (stored only).
        const ATTR_INIT_COMMAND = 1002;
        const ATTR_COMPRESS = 1003;
        const ATTR_DIRECT_QUERY = 1004;
        const ATTR_FOUND_ROWS = 1005;
        const ATTR_IGNORE_SPACE = 1006;
        const ATTR_SSL_KEY = 1007;
        const ATTR_SSL_CERT = 1008;
        const ATTR_SSL_CA = 1009;
        const ATTR_SSL_CAPATH = 1010;
        const ATTR_SSL_CIPHER = 1011;
        const ATTR_SERVER_PUBLIC_KEY = 1012;
        const ATTR_MULTI_STATEMENTS = 1013;
        const ATTR_SSL_VERIFY_SERVER_CERT = 1014;
        const ATTR_LOCAL_INFILE_DIRECTORY = 1015;

        public function __construct(string $dsn, ?string $username = null, ?string $password = null, ?array $options = null) {
            // F-CORE-01: this class had NO constructor at all, so `new Pdo\Mysql("sqlite:…")`
            // inherited \PDO's and cheerfully opened a SQLite database behind a Pdo\Mysql
            // object. The override exists solely to run the driver guard (and the `uri:`
            // resolution it depends on) before any connection is attempted; it adds no
            // MySQL-specific state of its own. See \PDO::checkDriverSubclassDsn().
            $_mysqlDsn = $this->resolveDsnUri($dsn);
            $this->checkDriverSubclassDsn($_mysqlDsn, "Pdo\\Mysql", "mysql");
            parent::__construct($_mysqlDsn, $username, $password, $options);
        }

        public function getWarningCount(): int {
            // The number of warnings raised by the last statement executed on this
            // connection (MySQL/MariaDB `@@warning_count`).
            return \elephc_pdo_warning_count($this->connectionId());
        }
    }

    class Pgsql extends \PDO {
        // PostgreSQL driver-specific constants (ext/pdo_pgsql). ATTR_* start at
        // PDO_ATTR_DRIVER_SPECIFIC (1000); TRANSACTION_* mirror libpq's PQTRANS_*
        // connection-transaction-status enum.
        const ATTR_DISABLE_PREPARES = 1000;
        const ATTR_RESULT_MEMORY_SIZE = 1001;
        const TRANSACTION_IDLE = 0;
        const TRANSACTION_ACTIVE = 1;
        const TRANSACTION_INTRANS = 2;
        const TRANSACTION_INERROR = 3;
        const TRANSACTION_UNKNOWN = 4;

        // Holds the compiled-PHP NOTICE callback (a no-op closure until one is
        // registered). Untyped and seeded with a closure so the checker infers it as a
        // callable that dynamic dispatch can invoke: a `?callable` property reads back
        // as a `Callable|Void` union (or a typed-array element as Mixed) that elephc's
        // checker will not narrow to a callable at the call site. Also the GC root that
        // keeps the closure reachable for the connection's lifetime.
        private $noticeCallback;

        public function __construct(string $dsn, ?string $username = null, ?string $password = null, ?array $options = null) {
            // F-CORE-01/F-CORE-11: resolve an indirect `uri:` DSN, then reject a DSN
            // belonging to another driver, both BEFORE any connection is attempted — see
            // \PDO::checkDriverSubclassDsn().
            $_pgsqlDsn = $this->resolveDsnUri($dsn);
            $this->checkDriverSubclassDsn($_pgsqlDsn, "Pdo\\Pgsql", "pgsql");
            // Forward to \PDO to open the connection, then seed a no-op callback so
            // drainNotices() always has a callable to hand each notice to (a notice
            // arriving before setNoticeCallback() is drained and discarded).
            parent::__construct($_pgsqlDsn, $username, $password, $options);
            $this->noticeCallback = function($_message) {};
        }

        public function setNoticeCallback(callable $callback): void {
            // Registers a callback invoked with the text of each PostgreSQL server
            // NOTICE. Divergences from PHP: (1) the parameter is a non-nullable
            // `callable` — elephc cannot yet narrow a nullable-callable property back to
            // a callable at the invocation site, so to stop delivery register a no-op
            // closure rather than passing null; (2) delivery is poll-based rather than
            // fired mid-protocol — the bridge buffers notices as they arrive (via the
            // connection's notice_callback) and this class drains + dispatches them
            // right after each exec()/query() on this connection (a NOTICE raised by a
            // prepared-statement execute() is delivered on the next exec()/query()). The
            // callback receives one string argument (the message); its return is ignored.
            $this->noticeCallback = $callback;
        }

        private function drainNotices(): void {
            // Hand every buffered NOTICE to the registered callback (a no-op until one
            // is set). A real server NOTICE always carries non-empty text, so an empty
            // return is the "no more pending" sentinel (as with the getNotify() drain).
            $_cb = $this->noticeCallback;
            while (true) {
                $_msg = \elephc_pdo_get_notice($this->connectionId());
                if ($_msg === "") {
                    break;
                }
                $_cb($_msg);
            }
        }

        public function exec(string $statement): int|bool {
            // Runs the statement through the base driver, then drains + dispatches any
            // server NOTICE it raised (e.g. a DO block / function using RAISE NOTICE).
            $_result = parent::exec($statement);
            $this->drainNotices();
            return $_result;
        }

        public function query(string $query, ?int $fetchMode = null, mixed $arg1 = null, mixed $arg2 = null): \PDOStatement|bool {
            // As exec(), but for a row-returning statement. `\PDOStatement` is
            // fully-qualified because this override lives inside `namespace Pdo`, where
            // a bare `PDOStatement` would resolve to the non-existent `Pdo\PDOStatement`.
            // Signature mirrors the widened base PDO::query() (P0-6) so overriding
            // stays arity-compatible; the extra args are simply forwarded.
            $_result = parent::query($query, $fetchMode, $arg1, $arg2);
            $this->drainNotices();
            return $_result;
        }

        public function escapeIdentifier(string $input): string {
            // PostgreSQL identifier quoting (PQescapeIdentifier semantics): double any
            // interior double-quote and wrap the whole identifier in double-quotes. A
            // pure string transform with no server round-trip, so it is safe to call
            // on any Pdo\Pgsql instance. (Divergence: PHP rejects an embedded NUL with
            // a ValueError; that pathological case is not guarded here.)
            $_doubled = \str_replace("\"", "\"\"", $input);
            return "\"" . $_doubled . "\"";
        }

        public function getPid(): int {
            // The PostgreSQL backend process id serving this connection
            // (`pg_backend_pid()`).
            return \elephc_pdo_backend_pid($this->connectionId());
        }

        public function lobCreate(): string|bool {
            // Creates an empty large object and returns its OID as a numeric string,
            // or false on error (PHP returns the OID as a string).
            $_oid = \elephc_pdo_lob_create($this->connectionId());
            return $_oid === "" ? false : $_oid;
        }

        public function lobUnlink(string $oid): bool {
            // Deletes the large object with the given OID.
            return \elephc_pdo_lob_unlink($this->connectionId(), $oid) === 1;
        }

        public function lobOpen(string $oid, string $mode = "rb"): mixed {
            // Opens a large object as a stream resource. Divergence from PHP: this is a
            // read-whole snapshot — the whole large object is read (SQL `lo_get`) into
            // an in-memory stream, so reads work fully but writes are not flushed back
            // to the object, and $mode is accepted for signature compatibility but not
            // otherwise honored (PHP opens "rb"/"wb" descriptors). Returns false if the
            // OID is non-numeric or no such large object exists.
            $_unused = $mode;
            $_len = \elephc_pdo_lob_get($this->connectionId(), $oid);
            return $this->blobStream($_len);
        }

        private function copyOptions(string $separator, string $nullAs): string {
            // PostgreSQL COPY text format defaults DELIMITER to a tab and NULL to
            // "\N", so only emit a WITH clause when the caller overrides them. A tab
            // delimiter must use the E'\t' escape-string form.
            //
            // F-PG-05: the separator is TRUNCATED TO ITS FIRST BYTE. PostgreSQL's COPY
            // grammar admits only a single one-byte delimiter, and all four of php-src's
            // COPY builders dereference exactly one byte of the argument —
            // `(pg_delim_len ? *pg_delim : '\t')` (pgsql_driver.c:654, 773, 882, 973) —
            // silently dropping the rest. This prelude interpolated the WHOLE string, so
            // `copyFromArray(…, "::")` emitted `DELIMITER '::'` and the SERVER rejected the
            // statement, where real PHP quietly copies with `:`. Truncating is not
            // "accepting garbage": it is the documented, observable behavior of the
            // function being reimplemented, and the alternative (a hard error) would fail
            // code that works on real PDO.
            //
            // An EMPTY separator falls back to the tab default, which is php-src's own
            // `pg_delim_len ? … : '\t'` ternary — the length test, not the byte.
            $_sep = $separator === "" ? "\t" : \substr($separator, 0, 1);
            if ($_sep === "\t" && $nullAs === "\\N") {
                return "";
            }
            $_delim = $_sep === "\t" ? "E'\\t'" : "'" . $_sep . "'";
            $_null = "'" . \str_replace("'", "''", $nullAs) . "'";
            return " WITH (DELIMITER " . $_delim . ", NULL " . $_null . ")";
        }

        private function copyTarget(string $tableName, ?string $fields): string {
            // The `table [(col, …)]` prefix shared by the COPY builders.
            if ($fields !== null) {
                return $tableName . " (" . $fields . ")";
            }
            return $tableName;
        }

        public function copyFromArray(string $tableName, array $rows, string $separator = "\t", string $nullAs = "\\N", ?string $fields = null): bool {
            // Each element of $rows is a full line (its fields already joined by
            // $separator); join them into the newline-terminated stream COPY FROM
            // STDIN consumes. On error the connection's errorInfo is set by the bridge.
            $_data = \implode("\n", $rows) . "\n";
            $_sql = "COPY " . $this->copyTarget($tableName, $fields) . " FROM STDIN"
                . $this->copyOptions($separator, $nullAs);
            return \elephc_pdo_copy_in($this->connectionId(), $_sql, $_data) >= 0;
        }

        public function copyFromFile(string $tableName, string $filename, string $separator = "\t", string $nullAs = "\\N", ?string $fields = null): bool {
            // Reads the client-side file and streams it as COPY FROM STDIN, matching
            // PHP's client-side file read.
            $_data = \file_get_contents($filename);
            if ($_data === false) {
                return false;
            }
            $_sql = "COPY " . $this->copyTarget($tableName, $fields) . " FROM STDIN"
                . $this->copyOptions($separator, $nullAs);
            // Cast to string: the checker does not narrow $_data out of string|false
            // after the `=== false` guard above, and copy_in's $data param is Str.
            return \elephc_pdo_copy_in($this->connectionId(), $_sql, (string) $_data) >= 0;
        }

        public function copyToArray(string $tableName, string $separator = "\t", string $nullAs = "\\N", ?string $fields = null): array|false {
            // Returns the table's rows, one array element per row (each keeping its
            // trailing newline, as PHP's copyToArray does). P2-i: copy_out() returns
            // "" for BOTH an empty COPY and a transport error, so an empty result is
            // no longer enough to tell them apart; the bridge always resets errcode
            // to 0 on success and sets it non-zero via fail() on error (checked
            // immediately after the call, so nothing else can have touched it in
            // between), which is exactly the distinction the stub's `array|false`
            // return type needs. A genuinely empty table still returns [].
            $_sql = "COPY " . $this->copyTarget($tableName, $fields) . " TO STDOUT"
                . $this->copyOptions($separator, $nullAs);
            $_raw = \elephc_pdo_copy_out($this->connectionId(), $_sql);
            if ($_raw === "") {
                if (\elephc_pdo_errcode($this->connectionId()) != 0) {
                    return false;
                }
                return [];
            }
            $_lines = \explode("\n", \rtrim($_raw, "\n"));
            $_out = [];
            foreach ($_lines as $_line) {
                $_out[] = $_line . "\n";
            }
            return $_out;
        }

        public function copyToFile(string $tableName, string $filename, string $separator = "\t", string $nullAs = "\\N", ?string $fields = null): bool {
            // Writes the table's COPY TO STDOUT output to the client-side file.
            // P2-i: the same empty-vs-error ambiguity as copyToArray() applies here —
            // without the errcode check, a failed COPY would still write an empty
            // file and report success.
            $_sql = "COPY " . $this->copyTarget($tableName, $fields) . " TO STDOUT"
                . $this->copyOptions($separator, $nullAs);
            $_raw = \elephc_pdo_copy_out($this->connectionId(), $_sql);
            if ($_raw === "" && \elephc_pdo_errcode($this->connectionId()) != 0) {
                return false;
            }
            return \file_put_contents($filename, $_raw) !== false;
        }

        public function getNotify(int $fetchMode = 0, int $timeoutMilliseconds = 0): mixed {
            // Polls for a pending LISTEN/NOTIFY notification, or an empty array if
            // none arrived within the timeout. Divergence from PHP: an empty array is
            // returned rather than false for "no notification" (both are falsy, so
            // `while ($n = $db->getNotify())` still terminates).
            //
            // P2-5: $fetchMode == PDO::FETCH_ASSOC (2) shapes the result as
            // ["message"=>channel, "pid"=>pid, "payload"=>payload] (php-src
            // pgsql_driver.c's assoc keys — "message" holds the channel name);
            // anything else keeps the numerically-indexed [0=>channel, 1=>pid,
            // 2=>payload] (FETCH_NUM) shape. The declared return type is `mixed`
            // rather than PHP's own `array` (already a pre-existing divergence here,
            // documented in docs/php/pdo.md): elephc's EIR array backend cannot unify
            // a string-keyed array literal with a positionally-keyed one as a single
            // `array`-typed return, but boxing through `mixed` (the same technique
            // `PDOStatement::fetch()` already relies on for its own FETCH_ASSOC vs
            // FETCH_NUM branches) sidesteps that and lets both shapes coexist.
            $_raw = \elephc_pdo_get_notify($this->connectionId(), $timeoutMilliseconds);
            // elephc's explode takes no limit argument, so a tab in the payload is
            // not preserved beyond its first segment (channel names and the pid never
            // contain tabs, and NOTIFY payloads virtually never do).
            if ($fetchMode == 2) {
                if ($_raw === "") {
                    return [];
                }
                $_parts = \explode("\t", $_raw);
                $_pid = isset($_parts[1]) ? (int) $_parts[1] : 0;
                $_payload = isset($_parts[2]) ? $_parts[2] : "";
                return ["message" => $_parts[0], "pid" => $_pid, "payload" => $_payload];
            }
            if ($_raw === "") {
                return [];
            }
            $_parts = \explode("\t", $_raw);
            $_pid = isset($_parts[1]) ? (int) $_parts[1] : 0;
            $_payload = isset($_parts[2]) ? $_parts[2] : "";
            return [$_parts[0], $_pid, $_payload];
        }
    }
}
"#;

/// Prepends the PDO prelude statements to `program` when it references PDO, so the
/// classes and `elephc_pdo` externs compile through the normal pipeline only
/// for PDO-using programs. The prelude carries only declarations (extern block +
/// classes), which are hoisted, so prepending them ahead of user code does not
/// change top-level execution order. The prelude is static and tested, so a
/// tokenize/parse failure is a compiler bug and panics rather than silently
/// degrading.
///
/// `force` (set by `--with-pdo`) bypasses the usage scan so the PDO surface is
/// always injected, making it available even when auto-detection would not see
/// the usage.
pub fn inject_if_used(program: Program, force: bool) -> Program {
    if !force && !detect::program_uses_pdo(&program) {
        return program;
    }
    let tokens = crate::lexer::tokenize(PDO_PRELUDE_SRC).expect("PDO prelude must tokenize");
    let mut combined = crate::parser::parse(&tokens).expect("PDO prelude must parse");
    combined.extend(program);
    combined
}
