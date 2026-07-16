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

use std::borrow::Cow;

use crate::parser::ast::Program;
use crate::php_version::PhpVersion;

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
    function elephc_pdo_available_driver_count(): int;
    function elephc_pdo_available_driver_name(int $index): string;
    function elephc_pdo_ini_dsn_defined(string $name): int;
    function elephc_pdo_ini_dsn_value(string $name): string;
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
    // TLS), applied to the mysql: ring-backed rustls backend (enabled by default;
    // custom minimal builds may omit `mysql-tls`); ignored for sqlite:/pgsql: DSNs — PostgreSQL carries its own
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
    function elephc_pdo_open_persistent(string $dsn, int $persistent, int $sqlite_flags, string $my_init_command, string $my_ssl_config, int $my_found_rows, string $persistent_key, string $my_driver_config): int;
    function elephc_pdo_last_open_error(): string;
    function elephc_pdo_close(int $conn): void;
    function elephc_pdo_release(int $conn, int $resetPgsqlSession): void;
    // v35: unregisters every SQLite native callback before PHP descriptor roots
    // are released, including when the native handle remains in the persistent pool.
    function elephc_pdo_clear_callbacks(int $conn): int;
    function elephc_pdo_exec(int $conn, string $sql): int;
    function elephc_pdo_last_insert_id(int $conn, string $name): int;
    function elephc_pdo_changes(int $conn): int;
    function elephc_pdo_begin(int $conn): int;
    function elephc_pdo_commit(int $conn): int;
    function elephc_pdo_rollback(int $conn): int;
    function elephc_pdo_errcode(int $conn): int;
    function elephc_pdo_errmsg(int $conn): string;
    function elephc_pdo_prepare(int $conn, string $sql, int $emulated): int;
    function elephc_pdo_bind_parameter_index(int $stmt, string $name): int;
    function elephc_pdo_bind_int(int $stmt, int $idx, int $val): int;
    function elephc_pdo_bind_double(int $stmt, int $idx, float $val): int;
    // v20 adds an explicit $len (the value's true byte length) to bind_text, so a
    // value with an embedded NUL byte binds in full instead of truncating at the
    // first NUL, and declares bind_blob (bridge-side since v7, but never called
    // from the prelude until now) so PDO::PARAM_LOB binds route to it.
    function elephc_pdo_bind_text(int $stmt, int $idx, string $val, int $len): int;
    // v32: MySQL national-character string binding for PARAM_STR_NATL/default mode.
    function elephc_pdo_bind_text_national(int $stmt, int $idx, string $val, int $len): int;
    function elephc_pdo_bind_blob(int $stmt, int $idx, string $data, int $len): int;
    function elephc_pdo_bind_null(int $stmt, int $idx): int;
    function elephc_pdo_reset(int $stmt): int;
    function elephc_pdo_clear_bindings(int $stmt): int;
    function elephc_pdo_step(int $stmt): int;
    // v37: PostgreSQL scroll-cursor movement using PDO::FETCH_ORI_* semantics.
    function elephc_pdo_step_oriented(int $stmt, int $orientation, int $offset): int;
    // v39: bytes owned by an executed PostgreSQL result.
    function elephc_pdo_result_memory_size(int $stmt): int;
    // v34: advances through every MySQL protocol result set retained at execute time.
    function elephc_pdo_next_rowset(int $stmt): int;
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
    function elephc_pdo_stmt_sent_sql(int $stmt): string;
    function elephc_pdo_bind_bool(int $stmt, int $idx, int $val): int;
    function elephc_pdo_set_busy_timeout(int $conn, int $ms): int;
    function elephc_pdo_server_version(int $conn): string;
    // ABI v36: the remaining generic PDO connection-information attributes.
    function elephc_pdo_client_version(int $conn): string;
    function elephc_pdo_server_info(int $conn): string;
    function elephc_pdo_connection_status(int $conn): string;
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
    // v12: whole-BLOB / legacy whole-large-object snapshots. blob_read (SQLite)
    // and lob_get (PostgreSQL compatibility) load the value into a shared buffer and return its
    // byte length (-1 on error); blob_byte reads one byte out of that
    // buffer. Since v24 the buffer is copied out in a single ptr_read_string through
    // blob_data_ptr (below) rather than drained a byte at a time, so blob_byte is now
    // only the fallback/compat accessor — both paths preserve embedded NUL bytes.
    function elephc_pdo_blob_read(int $conn, string $table, string $column, int $rowid, string $dbname): int;
    function elephc_pdo_lob_get(int $conn, string $oid): int;
    function elephc_pdo_blob_byte(int $offset): int;
    // v40: legacy whole-value binary-safe writeback for the internal seekable
    // BLOB/LOB wrappers. Both remain for ABI compatibility now that v45/v46
    // supply bounded PostgreSQL/SQLite operations.
    function elephc_pdo_blob_write(int $conn, string $table, string $column, int $rowid, string $dbname, string $data, int $len): int;
    function elephc_pdo_lob_put(int $conn, string $oid, string $data, int $len): int;
    // v45: bounded PostgreSQL large-object I/O. `lob_size` transfers only a
    // scalar; `lob_read_at` fills the shared blob buffer with one requested
    // slice; `lob_write_at` patches one slice at its server-side offset.
    function elephc_pdo_lob_size(int $conn, string $oid): int;
    function elephc_pdo_lob_read_at(int $conn, string $oid, int $offset, int $len): int;
    function elephc_pdo_lob_write_at(int $conn, string $oid, int $offset, string $data, int $len): int;
    // v46: bounded SQLite incremental-BLOB I/O. `blob_size` transfers only a
    // scalar; `blob_read_at` fills the shared blob buffer with one requested
    // slice; `blob_write_at` patches one fixed-size slice at its native offset.
    function elephc_pdo_blob_size(int $conn, string $table, string $column, int $rowid, string $dbname): int;
    function elephc_pdo_blob_read_at(int $conn, string $table, string $column, int $rowid, string $dbname, int $offset, int $len): int;
    function elephc_pdo_blob_write_at(int $conn, string $table, string $column, int $rowid, string $dbname, int $offset, string $data, int $len): int;
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
    // beginTransaction()'s already-active guard (P1-g). SQLite reads native
    // autocommit; PostgreSQL/MySQL use bridge-maintained state updated after every
    // successful control command. -1 is reserved for an unknown handle.
    function elephc_pdo_in_transaction(int $conn): int;
    // v31: live MySQL PDO::ATTR_AUTOCOMMIT mutation and state.
    function elephc_pdo_set_autocommit(int $conn, int $enabled): int;
    function elephc_pdo_autocommit(int $conn): int;
    // v38: live MySQL column table-prefix configuration.
    function elephc_pdo_set_fetch_table_names(int $conn, int $enabled): int;
    function elephc_pdo_fetch_table_names(int $conn): int;
    // v41: MySQL buffered-query default used by subsequently prepared statements.
    function elephc_pdo_set_buffered_query(int $conn, int $enabled): int;
    function elephc_pdo_buffered_query(int $conn): int;
    // v42: PostgreSQL connection default and prepare-local ATTR_PREFETCH.
    function elephc_pdo_set_prefetch(int $conn, int $enabled): int;
    function elephc_pdo_stmt_set_prefetch(int $stmt, int $enabled): int;
    // v47: PHP 8.5+ maps ATTR_PREFETCH=0 onto lazy simple-query consumption too.
    function elephc_pdo_stmt_enable_simple_streaming(int $stmt): int;
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
    // v43: native source-table names for all drivers and MySQL field flags.
    function elephc_pdo_column_table_name(int $stmt, int $i): string;
    function elephc_pdo_column_flags(int $stmt, int $i): int;
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
    // v29: PHP 8.5 SQLite transaction-mode and statement-state attributes.
    function elephc_pdo_set_transaction_mode(int $conn, int $mode): int;
    function elephc_pdo_transaction_mode(int $conn): int;
    function elephc_pdo_stmt_busy(int $stmt): int;
    function elephc_pdo_stmt_explain_mode(int $stmt): int;
    function elephc_pdo_stmt_set_explain_mode(int $stmt, int $mode): int;
    // v30: PHP 8.5 SQLite authorizer callback registration and nullable reset.
    function elephc_pdo_set_authorizer(int $conn, ptr $descriptor, ptr $adapter): int;
    function elephc_pdo_clear_authorizer(int $conn): int;
    // v33: deferred authorizer TypeError/ValueError classification.
    function elephc_pdo_take_authorizer_error(int $conn): int;
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
function pdo_drivers(): array {
    $_drivers = [];
    $_count = elephc_pdo_available_driver_count();
    for ($_index = 0; $_index < $_count; $_index++) {
        $_drivers[] = elephc_pdo_available_driver_name($_index);
    }
    return $_drivers;
}

// Maps a SQLSTATE to the human-readable class description PDO interpolates into a
// driver-error message — e.g. "General error" for HY000, so a failed sqlite query
// reads "SQLSTATE[HY000]: General error: 1 no such table: t" exactly like php-src.
// Mirrors the complete PHP 8.4 `pdo_sqlstate_state_to_description` table
// (ext/pdo/pdo_sqlstate.c); an unknown state degrades to php-src's own
// "<<Unknown error>>" fallback.
function __elephc_pdo_sqlstate_description_0(string $state): string {
    if ($state === "00000") { return "No error"; }
    if ($state === "01000") { return "Warning"; }
    if ($state === "01001") { return "Cursor operation conflict"; }
    if ($state === "01002") { return "Disconnect error"; }
    if ($state === "01003") { return "NULL value eliminated in set function"; }
    if ($state === "01004") { return "String data, right truncated"; }
    if ($state === "01006") { return "Privilege not revoked"; }
    if ($state === "01007") { return "Privilege not granted"; }
    if ($state === "01008") { return "Implicit zero bit padding"; }
    if ($state === "0100C") { return "Dynamic result sets returned"; }
    if ($state === "01P01") { return "Deprecated feature"; }
    if ($state === "01S00") { return "Invalid connection string attribute"; }
    if ($state === "01S01") { return "Error in row"; }
    if ($state === "01S02") { return "Option value changed"; }
    if ($state === "01S06") { return "Attempt to fetch before the result set returned the first rowset"; }
    if ($state === "01S07") { return "Fractional truncation"; }
    if ($state === "01S08") { return "Error saving File DSN"; }
    if ($state === "01S09") { return "Invalid keyword"; }
    if ($state === "02000") { return "No data"; }
    if ($state === "02001") { return "No additional dynamic result sets returned"; }
    if ($state === "03000") { return "Sql statement not yet complete"; }
    if ($state === "07002") { return "COUNT field incorrect"; }
    if ($state === "07005") { return "Prepared statement not a cursor-specification"; }
    if ($state === "07006") { return "Restricted data type attribute violation"; }
    if ($state === "07009") { return "Invalid descriptor index"; }
    if ($state === "07S01") { return "Invalid use of default parameter"; }
    if ($state === "08000") { return "Connection exception"; }
    if ($state === "08001") { return "Client unable to establish connection"; }
    if ($state === "08002") { return "Connection name in use"; }
    if ($state === "08003") { return "Connection does not exist"; }
    if ($state === "08004") { return "Server rejected the connection"; }
    if ($state === "08006") { return "Connection failure"; }
    if ($state === "08007") { return "Connection failure during transaction"; }
    if ($state === "08S01") { return "Communication link failure"; }
    if ($state === "09000") { return "Triggered action exception"; }
    if ($state === "0A000") { return "Feature not supported"; }
    if ($state === "0B000") { return "Invalid transaction initiation"; }
    if ($state === "0F000") { return "Locator exception"; }
    if ($state === "0F001") { return "Invalid locator specification"; }
    if ($state === "0L000") { return "Invalid grantor"; }
    if ($state === "0LP01") { return "Invalid grant operation"; }
    if ($state === "0P000") { return "Invalid role specification"; }
    return "";
}

function __elephc_pdo_sqlstate_description_2(string $state): string {
    if ($state === "21000") { return "Cardinality violation"; }
    if ($state === "21S01") { return "Insert value list does not match column list"; }
    if ($state === "21S02") { return "Degree of derived table does not match column list"; }
    if ($state === "22000") { return "Data exception"; }
    if ($state === "22001") { return "String data, right truncated"; }
    if ($state === "22002") { return "Indicator variable required but not supplied"; }
    if ($state === "22003") { return "Numeric value out of range"; }
    if ($state === "22004") { return "Null value not allowed"; }
    if ($state === "22005") { return "Error in assignment"; }
    if ($state === "22007") { return "Invalid datetime format"; }
    if ($state === "22008") { return "Datetime field overflow"; }
    if ($state === "22009") { return "Invalid time zone displacement value"; }
    if ($state === "2200B") { return "Escape character conflict"; }
    if ($state === "2200C") { return "Invalid use of escape character"; }
    if ($state === "2200D") { return "Invalid escape octet"; }
    if ($state === "2200F") { return "Zero length character string"; }
    if ($state === "2200G") { return "Most specific type mismatch"; }
    if ($state === "22010") { return "Invalid indicator parameter value"; }
    if ($state === "22011") { return "Substring error"; }
    if ($state === "22012") { return "Division by zero"; }
    if ($state === "22015") { return "Interval field overflow"; }
    if ($state === "22018") { return "Invalid character value for cast specification"; }
    if ($state === "22019") { return "Invalid escape character"; }
    if ($state === "2201B") { return "Invalid regular expression"; }
    if ($state === "2201E") { return "Invalid argument for logarithm"; }
    if ($state === "2201F") { return "Invalid argument for power function"; }
    if ($state === "2201G") { return "Invalid argument for width bucket function"; }
    if ($state === "22020") { return "Invalid limit value"; }
    if ($state === "22021") { return "Character not in repertoire"; }
    if ($state === "22022") { return "Indicator overflow"; }
    if ($state === "22023") { return "Invalid parameter value"; }
    if ($state === "22024") { return "Unterminated c string"; }
    if ($state === "22025") { return "Invalid escape sequence"; }
    if ($state === "22026") { return "String data, length mismatch"; }
    if ($state === "22027") { return "Trim error"; }
    if ($state === "2202E") { return "Array subscript error"; }
    if ($state === "22P01") { return "Floating point exception"; }
    if ($state === "22P02") { return "Invalid text representation"; }
    if ($state === "22P03") { return "Invalid binary representation"; }
    if ($state === "22P04") { return "Bad copy file format"; }
    if ($state === "22P05") { return "Untranslatable character"; }
    if ($state === "23000") { return "Integrity constraint violation"; }
    if ($state === "23001") { return "Restrict violation"; }
    if ($state === "23502") { return "Not null violation"; }
    if ($state === "23503") { return "Foreign key violation"; }
    if ($state === "23505") { return "Unique violation"; }
    if ($state === "23514") { return "Check violation"; }
    if ($state === "24000") { return "Invalid cursor state"; }
    if ($state === "25000") { return "Invalid transaction state"; }
    if ($state === "25001") { return "Active sql transaction"; }
    if ($state === "25002") { return "Branch transaction already active"; }
    if ($state === "25003") { return "Inappropriate access mode for branch transaction"; }
    if ($state === "25004") { return "Inappropriate isolation level for branch transaction"; }
    if ($state === "25005") { return "No active sql transaction for branch transaction"; }
    if ($state === "25006") { return "Read only sql transaction"; }
    if ($state === "25007") { return "Schema and data statement mixing not supported"; }
    if ($state === "25008") { return "Held cursor requires same isolation level"; }
    if ($state === "25P01") { return "No active sql transaction"; }
    if ($state === "25P02") { return "In failed sql transaction"; }
    if ($state === "25S01") { return "Transaction state"; }
    if ($state === "25S02") { return "Transaction is still active"; }
    if ($state === "25S03") { return "Transaction is rolled back"; }
    if ($state === "26000") { return "Invalid sql statement name"; }
    if ($state === "27000") { return "Triggered data change violation"; }
    if ($state === "28000") { return "Invalid authorization specification"; }
    if ($state === "2B000") { return "Dependent privilege descriptors still exist"; }
    if ($state === "2BP01") { return "Dependent objects still exist"; }
    if ($state === "2D000") { return "Invalid transaction termination"; }
    if ($state === "2F000") { return "Sql routine exception"; }
    if ($state === "2F002") { return "Modifying sql data not permitted"; }
    if ($state === "2F003") { return "Prohibited sql statement attempted"; }
    if ($state === "2F004") { return "Reading sql data not permitted"; }
    if ($state === "2F005") { return "Function executed no return statement"; }
    return "";
}

function __elephc_pdo_sqlstate_description_3(string $state): string {
    if ($state === "34000") { return "Invalid cursor name"; }
    if ($state === "38000") { return "External routine exception"; }
    if ($state === "38001") { return "Containing sql not permitted"; }
    if ($state === "38002") { return "Modifying sql data not permitted"; }
    if ($state === "38003") { return "Prohibited sql statement attempted"; }
    if ($state === "38004") { return "Reading sql data not permitted"; }
    if ($state === "39000") { return "External routine invocation exception"; }
    if ($state === "39001") { return "Invalid sqlstate returned"; }
    if ($state === "39004") { return "Null value not allowed"; }
    if ($state === "39P01") { return "Trigger protocol violated"; }
    if ($state === "39P02") { return "Srf protocol violated"; }
    if ($state === "3B000") { return "Savepoint exception"; }
    if ($state === "3B001") { return "Invalid savepoint specification"; }
    if ($state === "3C000") { return "Duplicate cursor name"; }
    if ($state === "3D000") { return "Invalid catalog name"; }
    if ($state === "3F000") { return "Invalid schema name"; }
    return "";
}

function __elephc_pdo_sqlstate_description_4(string $state): string {
    if ($state === "40000") { return "Transaction rollback"; }
    if ($state === "40001") { return "Serialization failure"; }
    if ($state === "40002") { return "Transaction integrity constraint violation"; }
    if ($state === "40003") { return "Statement completion unknown"; }
    if ($state === "40P01") { return "Deadlock detected"; }
    if ($state === "42000") { return "Syntax error or access violation"; }
    if ($state === "42501") { return "Insufficient privilege"; }
    if ($state === "42601") { return "Syntax error"; }
    if ($state === "42602") { return "Invalid name"; }
    if ($state === "42611") { return "Invalid column definition"; }
    if ($state === "42622") { return "Name too long"; }
    if ($state === "42701") { return "Duplicate column"; }
    if ($state === "42702") { return "Ambiguous column"; }
    if ($state === "42703") { return "Undefined column"; }
    if ($state === "42704") { return "Undefined object"; }
    if ($state === "42710") { return "Duplicate object"; }
    if ($state === "42712") { return "Duplicate alias"; }
    if ($state === "42723") { return "Duplicate function"; }
    if ($state === "42725") { return "Ambiguous function"; }
    if ($state === "42803") { return "Grouping error"; }
    if ($state === "42804") { return "Datatype mismatch"; }
    if ($state === "42809") { return "Wrong object type"; }
    if ($state === "42830") { return "Invalid foreign key"; }
    if ($state === "42846") { return "Cannot coerce"; }
    if ($state === "42883") { return "Undefined function"; }
    if ($state === "42939") { return "Reserved name"; }
    if ($state === "42P01") { return "Undefined table"; }
    if ($state === "42P02") { return "Undefined parameter"; }
    if ($state === "42P03") { return "Duplicate cursor"; }
    if ($state === "42P04") { return "Duplicate database"; }
    if ($state === "42P05") { return "Duplicate prepared statement"; }
    if ($state === "42P06") { return "Duplicate schema"; }
    if ($state === "42P07") { return "Duplicate table"; }
    if ($state === "42P08") { return "Ambiguous parameter"; }
    if ($state === "42P09") { return "Ambiguous alias"; }
    if ($state === "42P10") { return "Invalid column reference"; }
    if ($state === "42P11") { return "Invalid cursor definition"; }
    if ($state === "42P12") { return "Invalid database definition"; }
    if ($state === "42P13") { return "Invalid function definition"; }
    if ($state === "42P14") { return "Invalid prepared statement definition"; }
    if ($state === "42P15") { return "Invalid schema definition"; }
    if ($state === "42P16") { return "Invalid table definition"; }
    if ($state === "42P17") { return "Invalid object definition"; }
    if ($state === "42P18") { return "Indeterminate datatype"; }
    if ($state === "42S01") { return "Base table or view already exists"; }
    if ($state === "42S02") { return "Base table or view not found"; }
    if ($state === "42S11") { return "Index already exists"; }
    if ($state === "42S12") { return "Index not found"; }
    if ($state === "42S21") { return "Column already exists"; }
    if ($state === "42S22") { return "Column not found"; }
    if ($state === "44000") { return "WITH CHECK OPTION violation"; }
    return "";
}

function __elephc_pdo_sqlstate_description_5(string $state): string {
    if ($state === "53000") { return "Insufficient resources"; }
    if ($state === "53100") { return "Disk full"; }
    if ($state === "53200") { return "Out of memory"; }
    if ($state === "53300") { return "Too many connections"; }
    if ($state === "54000") { return "Program limit exceeded"; }
    if ($state === "54001") { return "Statement too complex"; }
    if ($state === "54011") { return "Too many columns"; }
    if ($state === "54023") { return "Too many arguments"; }
    if ($state === "55000") { return "Object not in prerequisite state"; }
    if ($state === "55006") { return "Object in use"; }
    if ($state === "55P02") { return "Cant change runtime param"; }
    if ($state === "55P03") { return "Lock not available"; }
    if ($state === "57000") { return "Operator intervention"; }
    if ($state === "57014") { return "Query canceled"; }
    if ($state === "57P01") { return "Admin shutdown"; }
    if ($state === "57P02") { return "Crash shutdown"; }
    if ($state === "57P03") { return "Cannot connect now"; }
    if ($state === "58030") { return "Io error"; }
    if ($state === "58P01") { return "Undefined file"; }
    if ($state === "58P02") { return "Duplicate file"; }
    return "";
}

function __elephc_pdo_sqlstate_description_f(string $state): string {
    if ($state === "F0000") { return "Config file error"; }
    if ($state === "F0001") { return "Lock file exists"; }
    return "";
}

function __elephc_pdo_sqlstate_description_h(string $state): string {
    if ($state === "HY000") { return "General error"; }
    if ($state === "HY001") { return "Memory allocation error"; }
    if ($state === "HY003") { return "Invalid application buffer type"; }
    if ($state === "HY004") { return "Invalid SQL data type"; }
    if ($state === "HY007") { return "Associated statement is not prepared"; }
    if ($state === "HY008") { return "Operation canceled"; }
    if ($state === "HY009") { return "Invalid use of null pointer"; }
    if ($state === "HY010") { return "Function sequence error"; }
    if ($state === "HY011") { return "Attribute cannot be set now"; }
    if ($state === "HY012") { return "Invalid transaction operation code"; }
    if ($state === "HY013") { return "Memory management error"; }
    if ($state === "HY014") { return "Limit on the number of handles exceeded"; }
    if ($state === "HY015") { return "No cursor name available"; }
    if ($state === "HY016") { return "Cannot modify an implementation row descriptor"; }
    if ($state === "HY017") { return "Invalid use of an automatically allocated descriptor handle"; }
    if ($state === "HY018") { return "Server declined cancel request"; }
    if ($state === "HY019") { return "Non-character and non-binary data sent in pieces"; }
    if ($state === "HY020") { return "Attempt to concatenate a null value"; }
    if ($state === "HY021") { return "Inconsistent descriptor information"; }
    if ($state === "HY024") { return "Invalid attribute value"; }
    if ($state === "HY090") { return "Invalid string or buffer length"; }
    if ($state === "HY091") { return "Invalid descriptor field identifier"; }
    if ($state === "HY092") { return "Invalid attribute/option identifier"; }
    if ($state === "HY093") { return "Invalid parameter number"; }
    if ($state === "HY095") { return "Function type out of range"; }
    if ($state === "HY096") { return "Invalid information type"; }
    if ($state === "HY097") { return "Column type out of range"; }
    if ($state === "HY098") { return "Scope type out of range"; }
    if ($state === "HY099") { return "Nullable type out of range"; }
    if ($state === "HY100") { return "Uniqueness option type out of range"; }
    if ($state === "HY101") { return "Accuracy option type out of range"; }
    if ($state === "HY103") { return "Invalid retrieval code"; }
    if ($state === "HY104") { return "Invalid precision or scale value"; }
    if ($state === "HY105") { return "Invalid parameter type"; }
    if ($state === "HY106") { return "Fetch type out of range"; }
    if ($state === "HY107") { return "Row value out of range"; }
    if ($state === "HY109") { return "Invalid cursor position"; }
    if ($state === "HY110") { return "Invalid driver completion"; }
    if ($state === "HY111") { return "Invalid bookmark value"; }
    if ($state === "HYC00") { return "Optional feature not implemented"; }
    if ($state === "HYT00") { return "Timeout expired"; }
    if ($state === "HYT01") { return "Connection timeout expired"; }
    return "";
}

function __elephc_pdo_sqlstate_description_i(string $state): string {
    if ($state === "IM001") { return "Driver does not support this function"; }
    if ($state === "IM002") { return "Data source name not found and no default driver specified"; }
    if ($state === "IM003") { return "Specified driver could not be loaded"; }
    if ($state === "IM004") { return "Driver's SQLAllocHandle on SQL_HANDLE_ENV failed"; }
    if ($state === "IM005") { return "Driver's SQLAllocHandle on SQL_HANDLE_DBC failed"; }
    if ($state === "IM006") { return "Driver's SQLSetConnectAttr failed"; }
    if ($state === "IM007") { return "No data source or driver specified; dialog prohibited"; }
    if ($state === "IM008") { return "Dialog failed"; }
    if ($state === "IM009") { return "Unable to load translation DLL"; }
    if ($state === "IM010") { return "Data source name too long"; }
    if ($state === "IM011") { return "Driver name too long"; }
    if ($state === "IM012") { return "DRIVER keyword syntax error"; }
    if ($state === "IM013") { return "Trace file error"; }
    if ($state === "IM014") { return "Invalid name of File DSN"; }
    if ($state === "IM015") { return "Corrupt file data source"; }
    return "";
}

function __elephc_pdo_sqlstate_description_p(string $state): string {
    if ($state === "P0000") { return "Plpgsql error"; }
    if ($state === "P0001") { return "Raise exception"; }
    return "";
}

function __elephc_pdo_sqlstate_description_x(string $state): string {
    if ($state === "XX000") { return "Internal error"; }
    if ($state === "XX001") { return "Data corrupted"; }
    return "";
}

function __elephc_pdo_sqlstate_description(string $state): string {
    $_prefix = substr($state, 0, 1);
    if ($_prefix === "0") {
        $_description = __elephc_pdo_sqlstate_description_0($state);
        if ($_description !== "") { return $_description; }
    }
    if ($_prefix === "2") {
        $_description = __elephc_pdo_sqlstate_description_2($state);
        if ($_description !== "") { return $_description; }
    }
    if ($_prefix === "3") {
        $_description = __elephc_pdo_sqlstate_description_3($state);
        if ($_description !== "") { return $_description; }
    }
    if ($_prefix === "4") {
        $_description = __elephc_pdo_sqlstate_description_4($state);
        if ($_description !== "") { return $_description; }
    }
    if ($_prefix === "5") {
        $_description = __elephc_pdo_sqlstate_description_5($state);
        if ($_description !== "") { return $_description; }
    }
    if ($_prefix === "F") {
        $_description = __elephc_pdo_sqlstate_description_f($state);
        if ($_description !== "") { return $_description; }
    }
    if ($_prefix === "H") {
        $_description = __elephc_pdo_sqlstate_description_h($state);
        if ($_description !== "") { return $_description; }
    }
    if ($_prefix === "I") {
        $_description = __elephc_pdo_sqlstate_description_i($state);
        if ($_description !== "") { return $_description; }
    }
    if ($_prefix === "P") {
        $_description = __elephc_pdo_sqlstate_description_p($state);
        if ($_description !== "") { return $_description; }
    }
    if ($_prefix === "X") {
        $_description = __elephc_pdo_sqlstate_description_x($state);
        if ($_description !== "") { return $_description; }
    }
    return "<<Unknown error>>";
}

// Formats a synthetic PDO implementation error exactly like php-src's
// `pdo_raise_impl_error`: the standard SQLSTATE description is always present,
// and caller detail is appended once only when non-empty.
function __elephc_pdo_impl_error_message(string $state, string $detail): string {
    $_message = "SQLSTATE[" . $state . "]: " . __elephc_pdo_sqlstate_description($state);
    if ($detail !== "") {
        return $_message . ": " . $detail;
    }
    return $_message;
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
    private string $sqlStateCode = "";

    // F-SURF-11: the previous exception in the chain. php-src keeps this in the base
    // Exception's private slot. elephc stores it here because the compiler-owned base
    // Throwable layout has no previous slot; PDOException's getPrevious() is deliberately
    // dispatched to the PHP method below instead of the generic null intrinsic.
    public ?Throwable $previous = null;

    // F-SURF-10/F-SURF-11: the public constructor matches inherited Exception. Structured
    // driver metadata is populated only through the private factory below, which the
    // checker exposes to PDO/PDOStatement prelude methods as an internal friend channel.
    // php-src stores the SQLSTATE string in the inherited code slot. elephc's base
    //    Exception slot is integer-only, so this class keeps the SQLSTATE in a dedicated
    //    string property and dispatches getCode() through the PDOException method below.
    //    The base integer slot still records errorInfo[1] for internal compatibility.
    public function __construct(string $message = "", int $code = 0, ?Throwable $previous = null) {
        // The built-in Exception constructor is a checker-synthesized method with
        // no linkable symbol, so `parent::__construct()` cannot be called; the
        // public `$message` property (see getMessage()) is assigned directly.
        $this->message = $message;
        $this->code = $code;
        $this->previous = $previous;
    }

    private static function __elephcFromErrorInfo(string $message, ?array $errorInfo = null, ?Throwable $previous = null): PDOException {
        $_error = new PDOException($message, 0, $previous);
        $_error->errorInfo = $errorInfo;
        // Keep both PDO's SQLSTATE code and the bridge's native integer code. is_array()
        // narrowing is used because errorInfo is nullable at connection-failure sites.
        if (is_array($errorInfo)) {
            if (count($errorInfo) > 0) {
                $_sqlState = $errorInfo[0];
                if (is_string($_sqlState)) {
                    $_error->sqlStateCode = (string) $_sqlState;
                }
            }
            if (count($errorInfo) > 1) {
                $_driverCode = $errorInfo[1];
                if (is_int($_driverCode)) {
                    $_error->code = (int) $_driverCode;
                }
            }
        }
        return $_error;
    }

    public function getCode(): string|int {
        if ($this->sqlStateCode !== "") {
            return $this->sqlStateCode;
        }
        return $this->code;
    }

    public function getPrevious(): ?Throwable {
        return $this->previous;
    }
}

// Compiler-owned wrapper behind Pdo\Sqlite::openBlob(). The native bridge keeps
// the database handle and performs bounded binary-safe fixed-size operations; this
// PHP object owns only the independently seekable cursor and current cell size.
final class __ElephcPDOSqliteBlobStream {
    private static bool $registered = false;
    private static int $pendingConn = 0;
    private static string $pendingTable = "";
    private static string $pendingColumn = "";
    private static int $pendingRowid = 0;
    private static string $pendingDbname = "main";
    private static int $pendingSize = 0;
    private static bool $pendingWritable = false;

    private int $conn = 0;
    private string $table = "";
    private string $column = "";
    private int $rowid = 0;
    private string $dbname = "main";
    private int $size = 0;
    private int $position = 0;
    private bool $writable = false;

    public static function create(int $conn, string $table, string $column, int $rowid, string $dbname, int $flags): mixed {
        $_size = elephc_pdo_blob_size($conn, $table, $column, $rowid, $dbname);
        if ($_size < 0) {
            return false;
        }
        if (!self::$registered) {
            self::$registered = stream_wrapper_register("elephcpdosqliteblob", self::class);
            if (!self::$registered) {
                return false;
            }
        }
        self::$pendingConn = $conn;
        self::$pendingTable = $table;
        self::$pendingColumn = $column;
        self::$pendingRowid = $rowid;
        self::$pendingDbname = $dbname;
        self::$pendingSize = $_size;
        self::$pendingWritable = (($flags & 2) !== 0 && ($flags & 1) === 0);
        return fopen("elephcpdosqliteblob://open", self::$pendingWritable ? "r+" : "r");
    }

    public function stream_open($path, $mode, $options, &$openedPath): bool {
        $_unusedPath = $path;
        $_unusedMode = $mode;
        $_unusedOptions = $options;
        $this->conn = self::$pendingConn;
        $this->table = self::$pendingTable;
        $this->column = self::$pendingColumn;
        $this->rowid = self::$pendingRowid;
        $this->dbname = self::$pendingDbname;
        $this->size = self::$pendingSize;
        $this->writable = self::$pendingWritable;
        $this->position = 0;
        return true;
    }

    public function stream_read(int $count): string {
        if ($count <= 0 || $this->position >= $this->size) {
            return "";
        }
        $_length = elephc_pdo_blob_read_at($this->conn, $this->table, $this->column, $this->rowid, $this->dbname, $this->position, $count);
        if ($_length <= 0) {
            return "";
        }
        $_chunk = ptr_read_string(elephc_pdo_blob_data_ptr(), $_length);
        $this->position = $this->position + $_length;
        return $_chunk;
    }

    public function stream_write(string $chunk): int {
        if (!$this->writable) {
            return -1;
        }
        $_count = strlen($chunk);
        if ($this->position + $_count > $this->size) {
            return -1;
        }
        $_written = elephc_pdo_blob_write_at($this->conn, $this->table, $this->column, $this->rowid, $this->dbname, $this->position, $chunk, $_count);
        if ($_written !== $_count) {
            return -1;
        }
        $this->position = $this->position + $_written;
        return $_written;
    }

    public function stream_tell(): int {
        return $this->position;
    }

    public function stream_eof(): bool {
        return $this->position >= $this->size;
    }

    public function stream_seek(int $offset, int $whence): bool {
        $_size = $this->size;
        if ($whence === 0) {
            $_target = $offset;
        } elseif ($whence === 1) {
            $_target = $this->position + $offset;
        } elseif ($whence === 2) {
            $_target = $_size + $offset;
        } else {
            return false;
        }
        if ($_target < 0) {
            $this->position = 0;
            return false;
        }
        if ($_target > $_size) {
            $this->position = $_size;
            return false;
        }
        $this->position = $_target;
        return true;
    }

    public function stream_stat(): array {
        return ["size" => $this->size];
    }

    public function stream_flush(): bool {
        return true;
    }

    public function stream_close(): void {}
}

// Compiler-owned wrapper behind Pdo\Pgsql::lobOpen(). It keeps only the cursor and
// size locally: reads fetch bounded `lo_get` slices and writes patch bounded `lo_put`
// slices, so memory usage follows the caller's chunk size rather than the whole LOB.
// PostgreSQL itself preserves sparse seek/extension and zero-fill semantics.
final class __ElephcPDOPgsqlLobStream {
    private static bool $registered = false;
    private static int $pendingConn = 0;
    private static string $pendingOid = "";
    private static int $pendingSize = 0;
    private static bool $pendingWritable = false;
    private static ?PDO $pendingOwner = null;

    private int $conn = 0;
    private string $oid = "";
    private int $size = 0;
    private int $position = 0;
    private bool $writable = false;
    private ?PDO $owner = null;

    public static function create(PDO $owner, int $conn, string $oid, string $mode): mixed {
        if (!$owner->inTransaction()) {
            return false;
        }
        $_size = elephc_pdo_lob_size($conn, $oid);
        if ($_size < 0) {
            return false;
        }
        if (!self::$registered) {
            self::$registered = stream_wrapper_register("elephcpdopgsqllob", self::class);
            if (!self::$registered) {
                return false;
            }
        }
        self::$pendingConn = $conn;
        self::$pendingOid = $oid;
        self::$pendingSize = $_size;
        self::$pendingWritable = (strpos($mode, "+") !== false || strpos($mode, "w") !== false);
        self::$pendingOwner = $owner;
        return fopen("elephcpdopgsqllob://open", self::$pendingWritable ? "r+" : "r");
    }

    public function stream_open($path, $mode, $options, &$openedPath): bool {
        $_unusedPath = $path;
        $_unusedMode = $mode;
        $_unusedOptions = $options;
        $this->conn = self::$pendingConn;
        $this->oid = self::$pendingOid;
        $this->size = self::$pendingSize;
        $this->writable = self::$pendingWritable;
        $this->owner = self::$pendingOwner;
        $this->position = 0;
        return true;
    }

    public function stream_read(int $count): string {
        if ($this->owner === null || !$this->owner->inTransaction()) {
            return "";
        }
        if ($count <= 0 || $this->position >= $this->size) {
            return "";
        }
        $_requested = $count;
        if ($this->position + $_requested > $this->size) {
            $_requested = $this->size - $this->position;
        }
        $_length = elephc_pdo_lob_read_at($this->conn, $this->oid, $this->position, $_requested);
        if ($_length < 0) {
            return "";
        }
        $_chunk = "";
        if ($_length > 0) {
            $_chunk = ptr_read_string(elephc_pdo_blob_data_ptr(), $_length);
        }
        $this->position = $this->position + strlen($_chunk);
        return $_chunk;
    }

    public function stream_write(string $chunk): int {
        if (!$this->writable || $this->owner === null || !$this->owner->inTransaction()) {
            return -1;
        }
        $_count = strlen($chunk);
        $_written = elephc_pdo_lob_write_at($this->conn, $this->oid, $this->position, $chunk, $_count);
        if ($_written < 0) {
            return -1;
        }
        $this->position = $this->position + $_written;
        if ($this->position > $this->size) {
            $this->size = $this->position;
        }
        return $_written;
    }

    public function stream_tell(): int {
        return $this->position;
    }

    public function stream_eof(): bool {
        return $this->position >= $this->size;
    }

    public function stream_seek(int $offset, int $whence): bool {
        if ($this->owner === null || !$this->owner->inTransaction()) {
            return false;
        }
        if ($whence === 0) {
            $_target = $offset;
        } elseif ($whence === 1) {
            $_target = $this->position + $offset;
        } elseif ($whence === 2) {
            $_target = $this->size + $offset;
        } else {
            return false;
        }
        if ($_target < 0) {
            return false;
        }
        $this->position = $_target;
        return true;
    }

    public function stream_stat(): array {
        return ["size" => $this->size];
    }

    public function stream_flush(): bool {
        return true;
    }

    public function stream_close(): void {}
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
    // php-src leaves the DBH error code uninitialized until a driver operation
    // has run. This distinguishes a fresh connection (`errorCode() === null`,
    // `errorInfo()[0] === ""`) from a successful operation (`"00000"`).
    private bool $hasOperation;
    private bool $inTxn;
    private bool $autoCommit;
    private int $defaultStrParam;
    private int $defaultFetchMode;
    // PDO::ATTR_STATEMENT_CLASS stores the canonical two-part configuration used by
    // prepare()/query(): index 0 is the PDOStatement-derived class name and optional
    // index 1 is the constructor-argument array. Keeping the optional index absent is
    // observable through getAttribute() and distinguishes "no ctor args supplied" from
    // an explicitly supplied empty array.
    private array $statementClassConfig;
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
    // Driver protocol selection. MySQL follows php-src's emulated-by-default
    // behavior; PostgreSQL defaults to native and can request its simple-query
    // path through either ATTR_EMULATE_PREPARES or ATTR_DISABLE_PREPARES.
    private bool $emulatePrepares;
    private bool $disablePrepares;
    // Operation label used to preserve php-src's active-method name when query()
    // delegates preparation to prepare(). It is reset as soon as prepare() starts.
    private string $prepareOperation;
    // Roots callbacks registered through PHP 8.4's legacy
    // PDO::sqliteCreate* driver-extension methods.
    protected array $pdoUdfCallbacks;

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
    protected static function resolveDsnUri(string $dsn, string $operation): string {
        if (!str_starts_with($dsn, "uri:")) {
            return $dsn . "";
        }
        $_uri = substr($dsn, 4);
        if (str_starts_with($_uri, "file://")) {
            $_uri = substr($_uri, 7);
        }
        $_uriHandle = fopen($_uri, "rb");
        if ($_uriHandle === false) {
            throw new PDOException($operation . "(): Argument #1 (\$dsn) must be a valid data source URI");
        }
        $_uriLine = fgets($_uriHandle);
        fclose($_uriHandle);
        if ($_uriLine === false) {
            // EOF on the very first read: the stream opened but is empty, which php-src
            // reports identically to an unopenable one (dsn_from_uri returns NULL for both).
            throw new PDOException($operation . "(): Argument #1 (\$dsn) must be a valid data source URI");
        }
        // Explicit cast: the checker does not narrow fgets()'s `string|false` out of the
        // `=== false` guard above (the same accepted gap copyFromFile() casts around for
        // file_get_contents).
        $_resolved = rtrim((string) $_uriLine, "\r\n");
        if (strpos($_resolved, ":") === false) {
            throw new PDOException($operation . "(): Argument #1 (\$dsn) must be a valid data source name (via URI)");
        }
        return $_resolved;
    }

    // PHP resolves a colonless constructor/factory DSN through the startup
    // configuration key `pdo.dsn.<name>` before URI handling and driver dispatch.
    // The bridge reads PHPRC/php.ini and PHP_INI_SCAN_DIR fragments at runtime so
    // aliases remain deployment configuration rather than compiler constants.
    protected static function resolveDsnAlias(string $dsn, string $operation): string {
        if (strpos($dsn, ":") !== false) {
            return $dsn . "";
        }
        $_key = "pdo.dsn." . $dsn;
        if (elephc_pdo_ini_dsn_defined($dsn) !== 1) {
            throw new PDOException($operation . "(): Argument #1 (\$dsn) must be a valid data source name");
        }
        $_resolved = elephc_pdo_ini_dsn_value($dsn);
        if (strpos($_resolved, ":") === false) {
            throw new PDOException("invalid data source name (via INI: " . $_key . ")");
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
    // Neither existed here originally: the constructor let the bridge fail the open
    // while PDO::connect() threw php-src's bare text, so one failure had two messages
    // inside one class and a colonless DSN got neither.
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
        // No driver attempted a connection here, so the public PHP-compatible constructor
        // is used directly and errorInfo remains null.
        if (strpos($dsn, ":") === false) {
            throw new PDOException("PDO::__construct(): Argument #1 (\$dsn) must be a valid data source name");
        }
        $_driver = substr($dsn, 0, (int) strpos($dsn, ":"));
        if (!in_array($_driver, pdo_drivers(), true)) {
            throw new PDOException("could not find driver");
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

    public function __construct(string $dsn, ?string $username = null, #[\SensitiveParameter] ?string $password = null, ?array $options = null) {
        // F-CORE-11 / F-CORE-13: resolve an indirect `uri:` DSN and validate the result
        // FIRST — php-src does both ahead of the options loop and the driver connect
        // (pdo_dbh.c:346-372). Every later DSN test in this method reads $_dsn, never the
        // raw $dsn parameter, which for a `uri:` DSN still says "uri:…".
        $_operation = get_class($this) . "::__construct";
        $_dsn = self::resolveDsnAlias($dsn, $_operation);
        $_dsn = self::resolveDsnUri($_dsn, $_operation);
        $this->checkDsnIsSupported($_dsn);
        $this->errMode = 2;
        $this->persistent = false;
        $this->attributes = [];
        $this->hasOperation = false;
        $this->inTxn = false;
        $this->autoCommit = true;
        $this->defaultStrParam = 0x20000000;
        $this->defaultFetchMode = 4;
        $this->statementClassConfig = ["PDOStatement"];
        $this->stringifyFetches = false;
        $this->attrCase = 0;
        $this->oracleNulls = 0;
        $this->emulatePrepares = substr($_dsn, 0, 6) === "mysql:";
        $this->disablePrepares = false;
        $this->prepareOperation = "PDO::prepare";
        $this->pdoUdfCallbacks = [];
        // P1-10: Pdo\Sqlite::ATTR_OPEN_FLAGS, read from $options here and applied
        // at the open call below. Its numeric value (1000) is PDO_ATTR_DRIVER_SPECIFIC
        // (see self::ATTR_DRIVER_SPECIFIC) — the same value MySQL/PostgreSQL use for
        // their own first driver-specific attribute, but this is harmless: the bridge
        // only consults $_openFlags for a `sqlite:` DSN and ignores it otherwise.
        $_openFlags = 0;
        // P1-9: Pdo\Mysql::ATTR_INIT_COMMAND (one SQL statement
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
        // for the other drivers. ATTR_SSL_CAPATH (1010) is adapted to a temporary
        // multi-PEM CA bundle; ATTR_SSL_CIPHER (1011) is forwarded to the driver-option
        // parser and fails explicitly because rustls exposes no PDO cipher-string mapping.
        // $_mySslVerify stays -1 ("unset") until an
        // explicit ATTR_SSL_VERIFY_SERVER_CERT is seen.
        $_mySslCa = "";
        $_mySslCert = "";
        $_mySslKey = "";
        $_mySslVerify = -1;
        // F-MY-06: Pdo\Mysql::ATTR_FOUND_ROWS (1005), threaded to the bridge's connect
        // path below. F-CORE-16: the user-supplied ATTR_PERSISTENT pool key ("" = the
        // plain boolean-persistent pool). Both are read from $options in the loop below.
        $_myFoundRows = 0;
        $_myBufferedQuery = 1;
        $_myLocalInfile = 0;
        $_myLocalInfileDirectory = "";
        $_myCompress = 0;
        $_myIgnoreSpace = 0;
        $_myMultiStatements = 1;
        $_mySslCapath = "";
        $_mySslCipher = "";
        $_myServerPublicKey = "";
        $_persistentKey = "";
        $_statementClassConfigured = false;
        // Constructor options affect the connection that is opened below, so
        // apply them before the bridge sees the DSN. In particular,
        // ATTR_PERSISTENT selects the bridge's process-local DSN pool.
        if ($options !== null) {
            foreach ($options as $_attr => $_val) {
                $_iattr = (int) $_attr;
                if ($_iattr == 0) {
                    // Only pdo_mysql exposes a live AUTOCOMMIT hook; retain the
                    // normalized option until the connection is open below.
                    $this->autoCommit = $this->attrBoolValue($_val);
                } elseif ($_iattr == 3) {
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
                } elseif ($_iattr == 13) {
                    $this->statementClassConfig = $this->validateStatementClassConfig($_val, false);
                    $_statementClassConfigured = true;
                } elseif ($_iattr == 19) {
                    // P1-h: same ATTR_DEFAULT_FETCH_MODE validation as setAttribute() below.
                    $_ctorFetchMode = $this->attrIntValue($_val);
                    $this->checkDefaultFetchMode($_ctorFetchMode);
                    $this->defaultFetchMode = $_ctorFetchMode;
                } elseif ($_iattr == 17) {
                    $this->stringifyFetches = $this->attrBoolValue($_val);
                } elseif ($_iattr == 21 && str_starts_with($_dsn, "mysql:")) {
                    $_defaultStringType = $this->attrIntValue($_val);
                    $this->defaultStrParam = ($_defaultStringType == 0x40000000) ? 0x40000000 : 0x20000000;
                } elseif ($_iattr == 20) {
                    $this->emulatePrepares = $this->attrBoolValue($_val);
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
                    if (substr($_dsn, 0, 7) === "sqlite:") {
                        $_openFlags = (int) $_val;
                    } elseif (substr($_dsn, 0, 6) === "pgsql:") {
                        $this->disablePrepares = $this->attrBoolValue($_val);
                    } elseif (substr($_dsn, 0, 6) === "mysql:") {
                        $_myBufferedQuery = $this->attrBoolValue($_val) ? 1 : 0;
                    }
                } elseif ($_iattr == 1004 && substr($_dsn, 0, 6) === "mysql:") {
                    $this->emulatePrepares = $this->attrBoolValue($_val);
                } elseif ($_iattr == 1001 && substr($_dsn, 0, 6) === "mysql:") {
                    $_myLocalInfile = $this->attrBoolValue($_val) ? 1 : 0;
                } elseif ($_iattr == 1002 && substr($_dsn, 0, 6) === "mysql:") {
                    $_myInitCommand = (string) $_val;
                } elseif ($_iattr == 1003 && substr($_dsn, 0, 6) === "mysql:") {
                    $_myCompress = $this->attrBoolValue($_val) ? 1 : 0;
                } elseif ($_iattr == 1005 && substr($_dsn, 0, 6) === "mysql:") {
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
                } elseif ($_iattr == 1006 && substr($_dsn, 0, 6) === "mysql:") {
                    $_myIgnoreSpace = $this->attrBoolValue($_val) ? 1 : 0;
                } elseif ($_iattr == 1009) {
                    $_mySslCa = (string) $_val;
                } elseif ($_iattr == 1008) {
                    $_mySslCert = (string) $_val;
                } elseif ($_iattr == 1007) {
                    $_mySslKey = (string) $_val;
                } elseif ($_iattr == 1014) {
                    $_mySslVerify = ((bool) $_val) ? 1 : 0;
                } elseif ($_iattr == 1010 && substr($_dsn, 0, 6) === "mysql:") {
                    $_mySslCapath = (string) $_val;
                } elseif ($_iattr == 1011 && substr($_dsn, 0, 6) === "mysql:") {
                    $_mySslCipher = (string) $_val;
                } elseif ($_iattr == 1012 && substr($_dsn, 0, 6) === "mysql:") {
                    $_myServerPublicKey = (string) $_val;
                } elseif ($_iattr == 1013 && substr($_dsn, 0, 6) === "mysql:") {
                    $_myMultiStatements = $this->attrBoolValue($_val) ? 1 : 0;
                } elseif ($_iattr == 1015 && substr($_dsn, 0, 6) === "mysql:") {
                    $_myLocalInfileDirectory = (string) $_val;
                }
                $this->attributes[$_iattr] = $_val;
            }
        }
        if ($_statementClassConfigured && $this->persistent) {
            throw new PDOException("SQLSTATE[HY000]: General error: PDO::ATTR_STATEMENT_CLASS cannot be used with persistent PDO instances");
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
        $_myEncode = function(string $_option): string {
            return str_replace("=", "%3D", str_replace(";", "%3B", str_replace("%", "%25", $_option)));
        };
        $_myDriverConfig = "local=" . $_myLocalInfile
            . ";dir=" . $_myEncode($_myLocalInfileDirectory)
            . ";compress=" . $_myCompress
            . ";ignore=" . $_myIgnoreSpace
            . ";multi=" . $_myMultiStatements
            . ";buffered=" . $_myBufferedQuery
            . ";capath=" . $_myEncode($_mySslCapath)
            . ";cipher=" . $_myEncode($_mySslCipher)
            . ";serverkey=" . $_myEncode($_myServerPublicKey) . ";";
        $this->conn = elephc_pdo_open_persistent($_dsn, $this->persistent ? 1 : 0, $_openFlags, $_myInitCommand, $_mySslConfig, $_myFoundRows, $_persistentKey, $_myDriverConfig);
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
            throw PDOException::__elephcFromErrorInfo("SQLSTATE[" . $_sqlstate . "]: " . $_openMsg, [$_sqlstate, null, $_openMsg]);
        }
        // Reset pooled MySQL sessions as well as new ones: a prior persistent
        // borrower may have disabled autocommit. Other drivers reject this
        // attribute through their own hooks and are deliberately untouched.
        if (str_starts_with($_dsn, "mysql:")) {
            if (elephc_pdo_set_autocommit($this->conn, $this->autoCommit ? 1 : 0) !== 1) {
                $this->fail(elephc_pdo_errmsg($this->conn));
            }
            if (isset($this->attributes[14])) {
                elephc_pdo_set_fetch_table_names($this->conn, $this->attrBoolValue($this->attributes[14]) ? 1 : 0);
            }
        } elseif (str_starts_with($_dsn, "sqlite:")) {
            if (isset($this->attributes[1002])) {
                elephc_pdo_set_extended_result_codes($this->conn, $this->attrBoolValue($this->attributes[1002]) ? 1 : 0);
            }
            if (isset($this->attributes[1005])) {
                $_constructorTransactionMode = $this->attrIntValue($this->attributes[1005]);
                if ($_constructorTransactionMode >= 0 && $_constructorTransactionMode <= 2) {
                    elephc_pdo_set_transaction_mode($this->conn, $_constructorTransactionMode);
                }
            }
        } elseif (str_starts_with($_dsn, "pgsql:") && isset($this->attributes[1])) {
            elephc_pdo_set_prefetch($this->conn, $this->attrBoolValue($this->attributes[1]) ? 1 : 0);
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
        $_native = elephc_pdo_errcode($this->conn);
        // php-src pdo_handle_error builds "SQLSTATE[%s]: %s: %d %s" (state,
        // description, native code, driver message); errorInfo keeps the raw
        // [state, native, message] triple frameworks read via $e->errorInfo.
        $_full = "SQLSTATE[" . $_sqlstate . "]: " . __elephc_pdo_sqlstate_description($_sqlstate) . ": " . $_native . " " . $message;
        if ($this->errMode == 2) {
            throw PDOException::__elephcFromErrorInfo($_full, [$_sqlstate, $_native, $message]);
        }
        fwrite(STDERR, "PDO error: " . $_full . "\n");
    }

    // PHP exceptions cannot unwind through SQLite's C authorizer frame. The bridge
    // therefore records invalid callback results and this outer PDO boundary raises
    // the same Error subclass/message once SQLite has safely returned.
    private function throwAuthorizerError(string $operation): void {
        $_authorizerError = elephc_pdo_take_authorizer_error($this->conn);
        if ($_authorizerError == 0) {
            return;
        }
        if ($_authorizerError == 1) {
            throw new ValueError($operation . "(): Return value of the authorizer callback must be one of Pdo\\Sqlite::OK, Pdo\\Sqlite::DENY, or Pdo\\Sqlite::IGNORE");
        }
        if ($_authorizerError == 2) {
            throw new Error($operation . "(): SQLite authorizer callback raised an exception");
        }
        $_returnedType = "object";
        if ($_authorizerError == 10) {
            $_returnedType = "null";
        } elseif ($_authorizerError == 11) {
            $_returnedType = "float";
        } elseif ($_authorizerError == 12) {
            $_returnedType = "string";
        } elseif ($_authorizerError == 13) {
            $_returnedType = "bool";
        } elseif ($_authorizerError == 14) {
            $_returnedType = "array";
        }
        throw new TypeError($operation . "(): Return value of the authorizer callback must be of type int, " . $_returnedType . " returned");
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
        $_full = __elephc_pdo_impl_error_message($sqlstate, $message);
        if ($this->errMode == 2) {
            throw PDOException::__elephcFromErrorInfo($_full, [$sqlstate, 0]);
        }
        fwrite(STDERR, "PDO error: " . $_full . "\n");
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

    // Validates both the connection-level ATTR_STATEMENT_CLASS value and a prepare()-local
    // override. The AOT helper returns enough metadata to mirror php-src's distinct errors
    // without exposing compiler class tables to PHP code. Abstract subclasses are accepted
    // here and rejected only by prepare(), matching object_init_ex() timing in php-src.
    private function validateStatementClassConfig(mixed $value, bool $fromSetAttribute): array {
        if (!is_array($value)) {
            if ($fromSetAttribute) {
                throw new TypeError("PDO::setAttribute(): Argument #2 (\$value) PDO::ATTR_STATEMENT_CLASS value must be of type array, " . $this->attrValueTypeName($value) . " given");
            }
            throw new TypeError("PDO::ATTR_STATEMENT_CLASS value must be of type array, " . $this->attrValueTypeName($value) . " given");
        }
        if (!array_key_exists(0, $value)) {
            if ($fromSetAttribute) {
                throw new ValueError("PDO::setAttribute(): Argument #2 (\$value) PDO::ATTR_STATEMENT_CLASS value must be an array with the format array(classname, constructor_args)");
            }
            throw new ValueError("PDO::ATTR_STATEMENT_CLASS value must be an array with the format array(classname, constructor_args)");
        }
        if (!is_string($value[0])) {
            if ($fromSetAttribute) {
                throw new TypeError("PDO::setAttribute(): Argument #2 (\$value) PDO::ATTR_STATEMENT_CLASS class must be a valid class");
            }
            throw new TypeError("PDO::ATTR_STATEMENT_CLASS class must be a valid class");
        }
        $_class = (string) $value[0];
        $_status = __elephc_pdo_statement_class_status($_class);
        if ($_status == 0) {
            if ($fromSetAttribute) {
                throw new TypeError("PDO::setAttribute(): Argument #2 (\$value) PDO::ATTR_STATEMENT_CLASS class must be a valid class");
            }
            throw new TypeError("PDO::ATTR_STATEMENT_CLASS class must be a valid class");
        }
        if ($_status == 1) {
            if ($fromSetAttribute) {
                throw new TypeError("PDO::setAttribute(): Argument #2 (\$value) PDO::ATTR_STATEMENT_CLASS class must be derived from PDOStatement");
            }
            throw new TypeError("PDO::ATTR_STATEMENT_CLASS class must be derived from PDOStatement");
        }
        if ($_status == 2) {
            if ($fromSetAttribute) {
                throw new TypeError("PDO::setAttribute(): Argument #2 (\$value) User-supplied statement class cannot have a public constructor");
            }
            throw new TypeError("User-supplied statement class cannot have a public constructor");
        }
        $_config = [$_class];
        if (array_key_exists(1, $value)) {
            if (!is_array($value[1])) {
                // php-src 8.0-8.6 accidentally names the outer attribute value here,
                // so the reported type is "array" even though index 1 is the offender.
                if ($fromSetAttribute) {
                    throw new TypeError("PDO::setAttribute(): Argument #2 (\$value) PDO::ATTR_STATEMENT_CLASS constructor_args must be of type ?array, array given");
                }
                throw new TypeError("PDO::ATTR_STATEMENT_CLASS constructor_args must be of type ?array, array given");
            }
            $_config[1] = $value[1];
        }
        return $_config;
    }

    public function setAttribute(int $attribute, $value): bool {
        $_driver = elephc_pdo_driver_name($this->conn);
        if ($attribute == 0 && $_driver === "mysql") {
            $_autocommit = $this->attrBoolValue($value);
            if (elephc_pdo_set_autocommit($this->conn, $_autocommit ? 1 : 0) !== 1) {
                $this->fail(elephc_pdo_errmsg($this->conn));
                return false;
            }
            $this->autoCommit = $_autocommit;
        } elseif ($attribute == 3) {
            // F-CORE-03: the shape check runs BEFORE the range check, exactly as
            // php-src's pdo_get_long_param() does — see attrIntValue() for why a
            // blind cast here was actively dangerous for ATTR_ERRMODE.
            $_attrErrMode = $this->attrIntValue($value);
            $this->checkErrMode($_attrErrMode);
            $this->errMode = $_attrErrMode;
        } elseif ($attribute == 13) {
            if ($this->persistent) {
                $this->failCode("HY000", "PDO::ATTR_STATEMENT_CLASS cannot be used with persistent PDO instances");
                return false;
            }
            $this->statementClassConfig = $this->validateStatementClassConfig($value, true);
        } elseif ($attribute == 2 && $_driver === "sqlite") {
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
        } elseif ($attribute == 21 && $_driver === "mysql") {
            $_defaultStringType = $this->attrIntValue($value);
            $this->defaultStrParam = ($_defaultStringType == 0x40000000) ? 0x40000000 : 0x20000000;
        } elseif ($attribute == 14 && $_driver === "mysql") {
            return elephc_pdo_set_fetch_table_names($this->conn, $this->attrBoolValue($value) ? 1 : 0) === 1;
        } elseif ($attribute == 1000 && $_driver === "mysql") {
            return elephc_pdo_set_buffered_query($this->conn, $this->attrBoolValue($value) ? 1 : 0) === 1;
        } elseif ($attribute == 1 && $_driver === "pgsql") {
            return elephc_pdo_set_prefetch($this->conn, $this->attrBoolValue($value) ? 1 : 0) === 1;
        } elseif ($attribute == 20) {
            if ($_driver !== "mysql" && $_driver !== "pgsql") {
                return false;
            }
            $this->emulatePrepares = $this->attrBoolValue($value);
        } elseif ($attribute == 8) {
            $_attrCase = $this->attrIntValue($value);
            $this->checkAttrCase($_attrCase);
            $this->attrCase = $_attrCase;
        } elseif ($attribute == 11) {
            $this->oracleNulls = $this->attrIntValue($value);
        } elseif ($attribute == 1002 && $_driver === "sqlite") {
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
        } elseif ($attribute == 1005 && $_driver === "sqlite") {
            // PHP 8.5 Pdo\Sqlite::ATTR_TRANSACTION_MODE. php-src accepts the ordinary
            // PDO integer coercions, but returns false without changing state outside 0..2.
            $_transactionMode = $this->attrIntValue($value);
            if ($_transactionMode < 0 || $_transactionMode > 2) {
                return false;
            }
            return elephc_pdo_set_transaction_mode($this->conn, $_transactionMode) === 1;
        } elseif ($attribute == 1000 && $_driver === "pgsql") {
            $this->disablePrepares = $this->attrBoolValue($value);
        } elseif ($attribute == 1004 && $_driver === "mysql") {
            $this->emulatePrepares = $this->attrBoolValue($value);
        } else {
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
            // active driver's hook governs support; numeric-range membership alone
            // never makes an attribute readable or writable.
            return false;
        }
        return true;
    }

    // Virtual compiler-internal hook. Pdo\Pgsql overrides it with the real drain;
    // PDOStatement calls through its PDO-typed owner and runtime dispatch reaches
    // that override only for PostgreSQL connections.
    protected function __elephcDrainPgsqlNotices(): void {}

    public function getAttribute(int $attribute): mixed {
        if ($attribute == 0 && elephc_pdo_driver_name($this->conn) === "mysql") {
            return elephc_pdo_autocommit($this->conn) === 1;
        }
        if ($attribute == 14 && elephc_pdo_driver_name($this->conn) === "mysql") {
            return elephc_pdo_fetch_table_names($this->conn) === 1;
        }
        if ($attribute == 1000 && elephc_pdo_driver_name($this->conn) === "mysql") {
            return elephc_pdo_buffered_query($this->conn) === 1;
        }
        if ($attribute == 3) {
            return $this->errMode;
        }
        if ($attribute == 12) {
            return $this->persistent;
        }
        if ($attribute == 13) {
            return $this->statementClassConfig;
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
        if ($attribute == 21 && elephc_pdo_driver_name($this->conn) === "mysql") {
            return $this->defaultStrParam;
        }
        if ($attribute == 20 && (elephc_pdo_driver_name($this->conn) === "mysql" || elephc_pdo_driver_name($this->conn) === "pgsql")) {
            return $this->emulatePrepares;
        }
        if ($attribute == 1000 && elephc_pdo_driver_name($this->conn) === "pgsql") {
            return $this->disablePrepares;
        }
        if ($attribute == 1004 && elephc_pdo_driver_name($this->conn) === "mysql") {
            return $this->emulatePrepares;
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
        if ($attribute == 1005 && elephc_pdo_driver_name($this->conn) === "sqlite") {
            return elephc_pdo_transaction_mode($this->conn);
        }
        if ($attribute == 5) {
            return elephc_pdo_client_version($this->conn);
        }
        if ($attribute == 6 && elephc_pdo_driver_name($this->conn) !== "sqlite") {
            $serverInfo = elephc_pdo_server_info($this->conn);
            if ($serverInfo === "") {
                $this->failCode("HY000", "failed to read server information");
                return false;
            }
            return $serverInfo;
        }
        if ($attribute == 7 && elephc_pdo_driver_name($this->conn) !== "sqlite") {
            return elephc_pdo_connection_status($this->conn);
        }
        // Pdo\Sqlite::ATTR_EXTENDED_RESULT_CODES is write-only. Its get hook returns
        // unsupported, so it deliberately falls through to IM001 like php-src.
        // F-CORE-05: php-src's getAttribute fall-through — IM001 "driver does not support
        // that attribute" once the generic switch AND the driver hook have both declined
        // (pdo_dbh.c's `case 0:` arm), returning FALSE (php-src's literal `RETURN_FALSE`,
        // not NULL). errMode-aware like every other synthetic failure: ERRMODE_SILENT and
        // ERRMODE_WARNING still get `false` back rather than a throw. Unlike setAttribute's
        // IM001 (see the divergence note there), THIS one is exactly what real PHP does:
        // `(new PDO("sqlite::memory:"))->getAttribute(9999)` on a real 8.5.6 CLI throws
        // `SQLSTATE[IM001] … driver does not support that attribute`.
        //
        $this->failCode("IM001", "driver does not support that attribute");
        return false;
    }

    public function exec(string $statement): int|bool {
        // F-CORE-21/P2-f: real PHP validates this before any driver call at all —
        // php-src's PHP_METHOD(PDO, exec) raises the ValueError from its own
        // argument check, exactly like the prepare() guard just below (which this
        // method was inconsistently missing, so `exec("")` reached the bridge).
        if ($statement === "") {
            throw new ValueError("PDO::exec(): Argument #1 (\$statement) must not be empty");
        }
        $this->hasOperation = true;
        $_affected = elephc_pdo_exec($this->conn, $statement);
        if ($_affected < 0) {
            $this->throwAuthorizerError("PDO::exec");
            $this->fail(elephc_pdo_errmsg($this->conn));
            return false;
        }
        return $_affected;
    }

    public function prepare(string $query, array $options = []): PDOStatement|bool {
        $_operation = $this->prepareOperation;
        $this->prepareOperation = "PDO::prepare";
        // P2-f: real PHP validates this before any driver call at all.
        if ($query === "") {
            throw new ValueError("PDO::prepare(): Argument #1 (\$query) must not be empty");
        }
        $_driver = elephc_pdo_driver_name($this->conn);
        $_statementConfig = $this->statementClassConfig;
        if (array_key_exists(13, $options)) {
            $_statementConfig = $this->validateStatementClassConfig($options[13], false);
        }
        $_statementClass = (string) $_statementConfig[0];
        $_statementStatus = __elephc_pdo_statement_class_status($_statementClass);
        if ($_statementStatus == 4 || $_statementStatus == 6) {
            throw new Error("Cannot instantiate abstract class " . $_statementClass);
        }
        $_hasStatementConstructor = $_statementStatus == 5;
        if (array_key_exists(1, $_statementConfig) && !$_hasStatementConstructor) {
            throw new Error("User-supplied statement does not accept constructor arguments");
        }
        $_emulated = $this->emulatePrepares;
        $_disable = $this->disablePrepares;
        $_scrollable = false;
        $_prefetchOverride = -1;
        if (array_key_exists(10, $options)) {
            $_cursorMode = (int) $options[10];
            if ($_driver === "sqlite" && $_cursorMode !== 0) {
                return false;
            }
            if ($_driver === "pgsql" && $_cursorMode === 1) {
                $_scrollable = true;
            }
        }
        if (isset($options[20])) {
            $_emulated = $this->attrBoolValue($options[20]);
        }
        if ($_driver === "pgsql" && array_key_exists(1, $options)) {
            $_prefetchOverride = $this->attrBoolValue($options[1]) ? 1 : 0;
        }
        if ($_driver === "pgsql" && isset($options[1000])) {
            $_disable = $this->attrBoolValue($options[1000]);
        }
        if ($_driver === "mysql" && isset($options[1004])) {
            $_emulated = $this->attrBoolValue($options[1004]);
        }
        $_simple = (($_driver === "mysql" && $_emulated) || ($_driver === "pgsql" && ($_emulated || $_disable || $_scrollable))) ? 1 : 0;
        $this->hasOperation = true;
        $_handle = elephc_pdo_prepare($this->conn, $query, $_simple);
        if ($_handle < 0) {
            $this->throwAuthorizerError($_operation);
            $this->fail(elephc_pdo_errmsg($this->conn));
            return false;
        }
        if ($_prefetchOverride != -1) {
            elephc_pdo_stmt_set_prefetch($_handle, $_prefetchOverride);
        }
        // -- elephc PHP >= 8.5 PDO pgsql simple streaming begin --
        if ($_driver === "pgsql") {
            elephc_pdo_stmt_enable_simple_streaming($_handle);
        }
        // -- elephc PHP >= 8.5 PDO pgsql simple streaming end --
        // Inherit the connection's default fetch mode (ATTR_DEFAULT_FETCH_MODE) so
        // a statement fetched with no explicit mode uses the dbh default.
        $_stmt = __elephc_new_without_constructor($_statementClass);
        __elephc_initialize_pdo_statement($_stmt, $_handle, $this->conn, $this->errMode, $query);
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
        $_stmt->setDefaultStrParam($this->defaultStrParam);
        // P1-i: snapshot ATTR_EMULATE_PREPARES the same way, so
        // PDOStatement::getAttribute(ATTR_EMULATE_PREPARES) answers from the
        // owning connection's stored value (or false when never set) instead of
        // raising IM001 like every other unsupported statement attribute.
        $_stmt->setEmulatePrepares($_simple === 1);
        // P2-e: snapshot ATTR_CASE / ATTR_ORACLE_NULLS the same way (see the
        // property comments on $attrCase/$oracleNulls above).
        $_stmt->setAttrCase($this->attrCase);
        $_stmt->setOracleNulls($this->oracleNulls);
        $_stmt->setScrollable($_scrollable);
        if ($_hasStatementConstructor) {
            if (array_key_exists(1, $_statementConfig)) {
                __elephc_invoke_pdo_statement_constructor($_statementClass, $_stmt, $_statementConfig[1]);
            } else {
                __elephc_invoke_pdo_statement_constructor($_statementClass, $_stmt, []);
            }
        }
        // The supported prepare-time protocol attributes were read explicitly above.
        // Other options remain driver-owned; no generic attribute bag is consulted.
        $_ignoredOptions = $options;
        return $_stmt;
    }

    public function query(string $query, ?int $fetchMode = null, mixed ...$fetchModeArgs): PDOStatement|bool {
        // F-CORE-22: php-src's PHP_METHOD(PDO, query) carries its OWN empty-statement
        // check, so this must not be left to the prepare() call below — an empty query
        // did throw, but under the wrong method name ("PDO::prepare(): ..."). php-src's
        // own message names the argument `$statement` here (the C-level parameter its
        // check validates) even though this prelude's parameter is `$query`; keep
        // php-src's text verbatim so a caller matching on the message sees real PHP's.
        if ($query === "") {
            throw new ValueError("PDO::query(): Argument #1 (\$statement) must not be empty");
        }
        $this->prepareOperation = "PDO::query";
        $_statement = $this->prepare($query);
        if ($_statement === false) {
            return false;
        }
        if ($_statement->execute() === false) {
            return false;
        }
        if ($fetchMode !== null) {
            // Explicit (int) cast: the checker does not narrow a `?int` parameter
            // to `int` from the `!== null` guard above when it flows into another
            // method call's argument, so an uncast $fetchMode fails to type-check
            // against setFetchMode()'s `int $mode` parameter.
            $_statement->setFetchMode((int) $fetchMode, ...$fetchModeArgs);
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
        $this->hasOperation = true;
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
        // PHP-side flag. SQLite reads native autocommit; PostgreSQL/MySQL expose
        // bridge-maintained state updated after every successful control command.
        // -1 remains the unknown-handle fallback.
        $_live = elephc_pdo_in_transaction($this->conn);
        $_alreadyActive = $_live === 1 || ($_live === -1 && $this->inTxn);
        if ($_alreadyActive) {
            throw new PDOException("There is already an active transaction");
        }
        if (elephc_pdo_begin($this->conn) != 1) {
            $this->hasOperation = true;
            $this->fail(elephc_pdo_errmsg($this->conn));
            return false;
        }
        $this->hasOperation = true;
        $this->inTxn = true;
        return true;
    }

    public function commit(): bool {
        // Committing without an active transaction is a logic error in PHP.
        if (!$this->inTransaction()) {
            throw new PDOException("There is no active transaction");
        }
        if (elephc_pdo_commit($this->conn) != 1) {
            $this->hasOperation = true;
            $this->fail(elephc_pdo_errmsg($this->conn));
            return false;
        }
        $this->hasOperation = true;
        $this->inTxn = false;
        return true;
    }

    public function rollBack(): bool {
        // Rolling back without an active transaction is a logic error in PHP.
        if (!$this->inTransaction()) {
            throw new PDOException("There is no active transaction");
        }
        if (elephc_pdo_rollback($this->conn) != 1) {
            $this->hasOperation = true;
            $this->fail(elephc_pdo_errmsg($this->conn));
            return false;
        }
        $this->hasOperation = true;
        $this->inTxn = false;
        return true;
    }

    public function inTransaction(): bool {
        // P1-g: prefer the driver's LIVE transaction state (matching php-src,
        // which asks the driver rather than trusting client-side bookkeeping) —
        // this is what makes a transaction started via a raw exec("BEGIN")
        // visible here for every supported driver. -1 is retained only as the
        // defensive unknown-handle fallback.
        $_live = elephc_pdo_in_transaction($this->conn);
        if ($_live === 0 || $_live === 1) {
            return $_live === 1;
        }
        return $this->inTxn;
    }

    public static function getAvailableDrivers(): array {
        $_drivers = [];
        $_count = elephc_pdo_available_driver_count();
        for ($_index = 0; $_index < $_count; $_index++) {
            $_drivers[] = elephc_pdo_available_driver_name($_index);
        }
        return $_drivers;
    }

    // -- elephc PHP >= 8.4 PDO::connect begin --
    public static function connect(string $dsn, ?string $username = null, #[\SensitiveParameter] ?string $password = null, ?array $options = null): static {
        // PHP 8.4 static factory: dispatch on the DSN driver prefix and return an
        // instance of the matching driver-specific subclass. Each subclass inherits
        // the whole \PDO surface, so the returned object opens the connection and
        // behaves exactly like `new PDO($dsn, ...)`; only its concrete class differs,
        // so `PDO::connect("sqlite:...") instanceof \Pdo\Sqlite` is true. Declared to
        // return the base \PDO because the subclasses ARE \PDO and elephc has no
        // `static` return type; the runtime object is the exact subclass. An
        // unrecognized prefix throws, matching PHP's "could not find driver".
        //
        $calledClass = static::class;
        $calledStatus = __elephc_pdo_called_class_status($calledClass);
        $_operation = $calledClass . "::connect";
        $_dsn = self::resolveDsnAlias($dsn, $_operation);
        $_dsn = self::resolveDsnUri($_dsn, $_operation);
        $_driver = "";
        $_driverClass = "";
        $_driverStatus = -1;
        if (str_starts_with($_dsn, "sqlite:")) {
            $_driver = "sqlite";
            $_driverClass = "Pdo\\Sqlite";
            $_driverStatus = 1;
        } elseif (str_starts_with($_dsn, "mysql:")) {
            $_driver = "mysql";
            $_driverClass = "Pdo\\Mysql";
            $_driverStatus = 2;
        } elseif (str_starts_with($_dsn, "pgsql:")) {
            $_driver = "pgsql";
            $_driverClass = "Pdo\\Pgsql";
            $_driverStatus = 3;
        }
        if ($_driver === "") {
            if ($calledStatus === 0) {
                throw new PDOException("could not find driver");
            }
            throw new PDOException($calledClass . "::connect() cannot be used for connecting to an unknown driver, call PDO::connect() instead");
        }
        if ($calledStatus === $_driverStatus) {
            return new static($_dsn, $username, $password, $options);
        }
        if ($calledStatus !== 0) {
            throw new PDOException($calledClass . "::connect() cannot be used for connecting to the \"" . $_driver . "\" driver, either call " . $_driverClass . "::connect() or PDO::connect() instead");
        }
        if ($_driverStatus === 1) {
            return new \Pdo\Sqlite($_dsn, $username, $password, $options);
        }
        if ($_driverStatus === 2) {
            return new \Pdo\Mysql($_dsn, $username, $password, $options);
        }
        return new \Pdo\Pgsql($_dsn, $username, $password, $options);
    }
    // -- elephc PHP >= 8.4 PDO::connect end --

    protected function connectionId(): int {
        // The raw bridge connection handle, exposed to driver subclasses (e.g.
        // Pdo\Pgsql::getPid, Pdo\Mysql::getWarningCount) so they can reach the
        // connection without widening the private $conn property. Called through
        // normal inherited method dispatch, so it reads $conn in the base class's
        // own scope.
        return $this->conn;
    }

    // PHP 8.4 still installs these three pdo_sqlite extension methods on the
    // base PDO class. Pdo\Sqlite exposes the modern spellings separately.
    public function sqliteCreateCollation(string $name, mixed $callback): bool {
        if (!is_callable($callback)) {
            throw new TypeError("PDO::sqliteCreateCollation(): Argument #2 (\$callback) must be a valid callback");
        }
        $_normalized = __elephc_normalize_callable($callback);
        $_descriptor = __elephc_callable_ptr($_normalized);
        $_adapter = __elephc_pdo_adapter_addr(0);
        if (elephc_pdo_create_collation($this->connectionId(), $name, $_descriptor, $_adapter) !== 1) {
            return false;
        }
        $this->pdoUdfCallbacks["collation:" . strtolower($name)] = $_normalized;
        return true;
    }

    public function sqliteCreateFunction(string $name, mixed $callback, int $numArgs = -1, int $flags = 0): bool {
        if (!is_callable($callback)) {
            throw new TypeError("PDO::sqliteCreateFunction(): Argument #2 (\$callback) must be a valid callback");
        }
        $_normalized = __elephc_normalize_callable($callback);
        $_descriptor = __elephc_callable_ptr($_normalized);
        $_adapter = __elephc_pdo_adapter_addr(1);
        if (elephc_pdo_create_function($this->connectionId(), $name, $numArgs, $flags, $_descriptor, $_adapter) !== 1) {
            return false;
        }
        $this->pdoUdfCallbacks["function:" . strtolower($name) . ":" . $numArgs . ":scalar"] = $_normalized;
        return true;
    }

    public function sqliteCreateAggregate(string $name, mixed $step, mixed $finalize, int $numArgs = -1): bool {
        if (!is_callable($step) || !is_callable($finalize)) {
            throw new TypeError("PDO::sqliteCreateAggregate(): step and finalize must be valid callbacks");
        }
        $_normalizedStep = __elephc_normalize_callable($step);
        $_normalizedFinal = __elephc_normalize_callable($finalize);
        $_stepDesc = __elephc_callable_ptr($_normalizedStep);
        $_stepAdapter = __elephc_pdo_adapter_addr(2);
        $_finalDesc = __elephc_callable_ptr($_normalizedFinal);
        $_finalAdapter = __elephc_pdo_adapter_addr(3);
        if (elephc_pdo_create_aggregate($this->connectionId(), $name, $numArgs, $_stepDesc, $_stepAdapter, $_finalDesc, $_finalAdapter) !== 1) {
            return false;
        }
        $_rootKey = "function:" . strtolower($name) . ":" . $numArgs;
        $this->pdoUdfCallbacks[$_rootKey . ":step"] = $_normalizedStep;
        $this->pdoUdfCallbacks[$_rootKey . ":final"] = $_normalizedFinal;
        return true;
    }

    // Shared PostgreSQL COPY SQL fragments for PHP 8.4's legacy PDO::pgsql*
    // extension methods.
    private function pdoPgsqlCopyOptions(string $separator, string $nullAs): string {
        $_sep = $separator === "" ? "\t" : substr($separator, 0, 1);
        if ($_sep === "\t" && $nullAs === "\\N") {
            return "";
        }
        $_delim = $_sep === "\t" ? "E'\\t'" : "'" . $_sep . "'";
        $_null = "'" . str_replace("'", "''", $nullAs) . "'";
        return " WITH (DELIMITER " . $_delim . ", NULL " . $_null . ")";
    }

    private function pdoPgsqlCopyTarget(string $tableName, ?string $fields): string {
        if ($fields !== null) {
            return $tableName . " (" . $fields . ")";
        }
        return $tableName;
    }

    public function pgsqlCopyFromArray(string $tableName, array $rows, string $separator = "\t", string $nullAs = "\\N", ?string $fields = null): bool {
        $_data = implode("\n", $rows) . "\n";
        $_sql = "COPY " . $this->pdoPgsqlCopyTarget($tableName, $fields) . " FROM STDIN"
            . $this->pdoPgsqlCopyOptions($separator, $nullAs);
        return elephc_pdo_copy_in($this->connectionId(), $_sql, $_data) >= 0;
    }

    public function pgsqlCopyFromFile(string $tableName, string $filename, string $separator = "\t", string $nullAs = "\\N", ?string $fields = null): bool {
        $_data = file_get_contents($filename);
        if ($_data === false) {
            return false;
        }
        $_sql = "COPY " . $this->pdoPgsqlCopyTarget($tableName, $fields) . " FROM STDIN"
            . $this->pdoPgsqlCopyOptions($separator, $nullAs);
        return elephc_pdo_copy_in($this->connectionId(), $_sql, (string) $_data) >= 0;
    }

    public function pgsqlCopyToArray(string $tableName, string $separator = "\t", string $nullAs = "\\N", ?string $fields = null): array|false {
        $_sql = "COPY " . $this->pdoPgsqlCopyTarget($tableName, $fields) . " TO STDOUT"
            . $this->pdoPgsqlCopyOptions($separator, $nullAs);
        $_raw = elephc_pdo_copy_out($this->connectionId(), $_sql);
        if ($_raw === "") {
            if (elephc_pdo_errcode($this->connectionId()) != 0) {
                return false;
            }
            return [];
        }
        $_lines = explode("\n", rtrim($_raw, "\n"));
        $_out = [];
        foreach ($_lines as $_line) {
            $_out[] = $_line . "\n";
        }
        return $_out;
    }

    public function pgsqlCopyToFile(string $tableName, string $filename, string $separator = "\t", string $nullAs = "\\N", ?string $fields = null): bool {
        $_sql = "COPY " . $this->pdoPgsqlCopyTarget($tableName, $fields) . " TO STDOUT"
            . $this->pdoPgsqlCopyOptions($separator, $nullAs);
        $_raw = elephc_pdo_copy_out($this->connectionId(), $_sql);
        if ($_raw === "" && elephc_pdo_errcode($this->connectionId()) != 0) {
            return false;
        }
        return file_put_contents($filename, $_raw) !== false;
    }

    public function pgsqlLOBCreate(): string|bool {
        if (!$this->inTransaction()) {
            return false;
        }
        $_oid = elephc_pdo_lob_create($this->connectionId());
        return $_oid === "" ? false : $_oid;
    }

    public function pgsqlLOBOpen(string $oid, string $mode = "rb"): mixed {
        return __ElephcPDOPgsqlLobStream::create($this, $this->connectionId(), $oid, $mode);
    }

    public function pgsqlLOBUnlink(string $oid): bool {
        if (!$this->inTransaction()) {
            return false;
        }
        return elephc_pdo_lob_unlink($this->connectionId(), $oid) === 1;
    }

    public function pgsqlGetNotify(int $fetchMode = 0, int $timeoutMilliseconds = 0): mixed {
        $_raw = elephc_pdo_get_notify($this->connectionId(), $timeoutMilliseconds);
        if ($_raw === "") {
            return false;
        }
        $_parts = explode("\t", $_raw);
        $_pid = isset($_parts[1]) ? (int) $_parts[1] : 0;
        $_payload = isset($_parts[2]) ? $_parts[2] : "";
        if ($fetchMode == 2) {
            return ["message" => $_parts[0], "pid" => $_pid, "payload" => $_payload];
        }
        return [$_parts[0], $_pid, $_payload];
    }

    public function pgsqlGetPid(): int {
        return elephc_pdo_backend_pid($this->connectionId());
    }

    public function errorCode(): ?string {
        // The 5-character SQLSTATE for the connection's last operation. php-src
        // returns null before the first operation and "00000" after a success.
        if (!$this->hasOperation) {
            return null;
        }
        return elephc_pdo_sqlstate($this->conn);
    }

    public function errorInfo(): array {
        // PHP's errorInfo() is [SQLSTATE, driver-specific code, message], with
        // ["00000", null, null] on success. Every driver surfaces a real SQLSTATE:
        // SQLite via a php-src-matching table, MySQL from the ERR packet's
        // #-marked field, PostgreSQL from the ErrorResponse 'C' field.
        if (!$this->hasOperation) {
            return ["", null, null];
        }
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
        $this->hasOperation = true;
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
        if ($this->inTxn || elephc_pdo_in_transaction($this->conn) === 1) {
            elephc_pdo_rollback($this->conn);
            $this->inTxn = false;
        }
        // Native SQLite registrations contain raw pointers into compiled-PHP
        // descriptors. Remove them before object-field cleanup releases the roots;
        // persistent bridge handles deliberately survive their final-owner release.
        elephc_pdo_clear_callbacks($this->conn);
        // -- elephc PHP >= 8.6 persistent pgsql reset --
        elephc_pdo_release($this->conn, 0);
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

// PHP's internal FETCH_LAZY row object is represented in userland here because the
// compiler has no native-class registration channel. The object is statement-owned and
// refreshed in place on every lazy fetch, so retained aliases observe the current row just
// like php-src's single `stmt->lazy_object_ref`. Magic property and ArrayAccess dispatch
// defer value lookup until access time. The two `__elephc*` entry points are necessarily
// public so PDOStatement can construct/refresh the object; that small reflection difference
// is preferable to silently substituting an eager array or rejecting a supported fetch mode.
final class PDORow implements ArrayAccess {
    public readonly string $queryString;
    private array $columns;
    private array $names;

    private function __construct(bool $internal = false, string $queryString = "") {
        if (!$internal) {
            throw new PDOException("You may not create a PDORow manually");
        }
        $this->queryString = $queryString;
        $this->columns = [];
        $this->names = [];
    }

    private function __elephcRefresh(array $columns, array $names): void {
        $this->columns = $columns;
        $this->names = $names;
    }

    public function __get(string $name): mixed {
        if (is_numeric($name)) {
            return $this->offsetGet((int) $name);
        }
        $_count = count($this->names);
        for ($_i = 0; $_i < $_count; $_i++) {
            if ($this->names[$_i] === $name) {
                return $this->columns[$_i];
            }
        }
        return null;
    }

    public function __isset(string $name): bool {
        return $this->__get($name) !== null;
    }

    public function __set(string $name, mixed $value): void {
        $_unusedName = $name;
        $_unusedValue = $value;
        throw new Error("Cannot write to PDORow property");
    }

    public function __unset(string $name): void {
        $_unusedName = $name;
        throw new Error("Cannot unset PDORow property");
    }

    public function offsetExists(mixed $offset): bool {
        return $this->offsetGet($offset) !== null;
    }

    public function offsetGet(mixed $offset): mixed {
        if (is_int($offset)) {
            $_index = (int) $offset;
            if ($_index >= 0 && $_index < count($this->columns)) {
                return $this->columns[$_index];
            }
            return null;
        }
        return $this->__get((string) $offset);
    }

    public function offsetSet(mixed $offset, mixed $value): void {
        $_unusedValue = $value;
        if ($offset === null) {
            throw new Error("Cannot append to PDORow offset");
        }
        throw new Error("Cannot write to PDORow offset");
    }

    public function offsetUnset(mixed $offset): void {
        $_unusedOffset = $offset;
        throw new Error("Cannot unset PDORow offset");
    }

    public function __serialize(): array {
        throw new Exception("Serialization of 'PDORow' is not allowed");
    }

    public function __sleep(): array {
        throw new Exception("Serialization of 'PDORow' is not allowed");
    }
}

// PHP exposes PDOStatement through IteratorAggregate. The prefixed helper owns its own
// row/key cursor state and delegates only to PDOStatement::fetch(), so PDOStatement does
// not leak Iterator's rewind/current/key/next/valid methods into its public API.
class PDOStatement implements IteratorAggregate {
    private int $stmt;
    private int $conn;
    private int $errMode;
    private int $fetchMode;
    private $fetchTarget;
    private array $fetchCtorArgs;
    private bool $fetchPropsLate;
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
    // Append-only bind indexes seen by the driver's execute-time normalization
    // hook. Named binds report paramno=-1 until their index appears here.
    private array $boundNormalizedIndexes;
    // bindParam() reference getters, keyed by the append index in boundValues.
    private array $boundParamRefIndexes;
    private array $boundParamRefGetters;
    // bindColumn() keeps its destination alive through a by-reference closure capture.
    // Parallel indexed arrays avoid heterogeneous records. Later duplicate keys shadow
    // earlier registrations during fetch, matching php-src's replacement semantics.
    private array $boundColumnKinds;
    private array $boundColumnIndexes;
    private array $boundColumnNames;
    private array $boundColumnSetters;
    private array $boundColumnTypes;
    private int $fetchColumn;
    private int $rowCount;
    private bool $executed;
    private bool $hasOperation;
    private mixed $lazyRow;
    // P1-4: mirrors php-src's pdo_sqlite `pre_fetched` flag — execute() eagerly
    // steps a SELECT-style statement once (see execute()'s comment) so
    // getColumnMeta() called before any explicit fetch() reports the real
    // column types of the first row instead of "no row yet". $pendingStep
    // caches that first step's result (elephc_pdo_step()'s return code) so the
    // FIRST subsequent stepCursor() call (from fetch()/fetchColumn()/etc.)
    // consumes it instead of stepping again, which would otherwise skip row 1.
    private bool $hasPendingStep;
    private int $pendingStep;
    // PostgreSQL `prepare(..., [PDO::ATTR_CURSOR => PDO::CURSOR_SCROLL])` enables
    // FETCH_ORI_* movement. SQLite rejects the option; MySQL remains forward-only.
    private bool $scrollable;
    // F-STMT-13: php-src makes $queryString read-only through a custom property-write
    // handler (dbstmt_prop_write: `zend_throw_error(NULL, "Property queryString is read
    // only")`), so `$stmt->queryString = 'x'` is an Error, not a silent overwrite of the
    // SQL the object reports. elephc has no property-write hook, but it DOES have
    // `readonly`: assignable once from the declaring class's constructor (the only place
    // this is written — see __construct), rejected everywhere else. The SQL a statement
    // reports can therefore never be overwritten, which is the point of the finding.
    //
    // Both a concrete PDOStatement receiver and the `PDOStatement|bool` union returned
    // by prepare()/query() raise the catchable Error. The text is PHP's generic readonly
    // message ("Cannot modify readonly property PDOStatement::$queryString") rather than
    // pdo_stmt.c's custom "Property queryString is read only"; class and catchability match.
    public readonly string $queryString;
    // Fallback copies used only before setOwner(); normal statements read the
    // owning PDO's live values at fetch/description time like php-src's stmt->dbh.
    private bool $stringifyFetches;
    // MySQL's default string-parameter flag, snapshotted from the connection.
    // 0x40000000 selects national `N'…'`; 0x20000000 selects ordinary text.
    private int $defaultStrParam;
    // P1-i: mirrors PDO::ATTR_EMULATE_PREPARES, snapshotted at prepare() time
    // from the owning connection's stored value (see setEmulatePrepares()).
    // getAttribute() answers this one attribute from the snapshot instead of
    // raising IM001 like every other unsupported statement attribute — no real
    // per-statement attribute store exists any more (setAttribute() always
    // fails; see its own comment).
    private bool $emulatePrepares;
    // Fallback copies of PDO::ATTR_CASE / ATTR_ORACLE_NULLS for the brief
    // pre-owner initialization path. Normal statement reads stay connection-live.
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
        $this->__elephcInitialize($handle, $connection, $errMode, $query);
    }

    // Internal initialization entry used after ATTR_STATEMENT_CLASS allocates a subclass
    // without invoking its user constructor. php-src fills the native statement fields and
    // queryString first, then invokes the protected/private constructor with user arguments.
    private function __elephcInitialize(int $handle, int $connection, int $errMode = 2, string $query = ""): void {
        $this->stmt = $handle;
        $this->conn = $connection;
        $this->errMode = $errMode;
        // PHP exposes the prepared SQL as the public PDOStatement::$queryString
        // property; thread it through from prepare() so debugDumpParams and callers
        // can read it.
        $this->queryString = $query;
        $this->fetchMode = 4;
        $this->fetchTarget = null;
        $this->fetchCtorArgs = [];
        $this->fetchPropsLate = false;
        $this->boundParams = [];
        $this->boundNames = [];
        $this->boundValues = [];
        $this->boundTypes = [];
        $this->boundPhpTypes = [];
        $this->boundNormalizedIndexes = [];
        $this->boundParamRefIndexes = [];
        $this->boundParamRefGetters = [];
        $this->boundColumnKinds = [];
        $this->boundColumnIndexes = [];
        $this->boundColumnNames = [];
        $this->boundColumnSetters = [];
        $this->boundColumnTypes = [];
        $this->fetchColumn = 0;
        $this->rowCount = 0;
        // Guards fetch*() against stepping a never-executed statement (which would
        // silently run the query with NULL binds). Set true by execute(), cleared
        // by closeCursor().
        $this->executed = false;
        $this->hasOperation = false;
        $this->lazyRow = null;
        $this->hasPendingStep = false;
        $this->pendingStep = 0;
        $this->scrollable = false;
        $this->stringifyFetches = false;
        $this->defaultStrParam = 0x20000000;
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

    public function setDefaultStrParam(int $type): void {
        $this->defaultStrParam = $type;
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

    private function currentStringifyFetches(): bool {
        if ($this->owner !== null) {
            return (bool) $this->owner->getAttribute(PDO::ATTR_STRINGIFY_FETCHES);
        }
        return $this->stringifyFetches;
    }

    private function currentAttrCase(): int {
        if ($this->owner !== null) {
            return (int) $this->owner->getAttribute(PDO::ATTR_CASE);
        }
        return $this->attrCase;
    }

    private function currentOracleNulls(): int {
        if ($this->owner !== null) {
            return (int) $this->owner->getAttribute(PDO::ATTR_ORACLE_NULLS);
        }
        return $this->oracleNulls;
    }

    public function setScrollable(bool $scrollable): void {
        $this->scrollable = $scrollable;
    }

    private function fail(string $message): void {
        // Per-statement error state (W1): the SQLSTATE, native code, and message
        // are read from the statement's own error slots and attached to errorInfo.
        if ($this->errMode == 0) {
            return;
        }
        $_sqlstate = elephc_pdo_stmt_sqlstate($this->stmt);
        $_native = elephc_pdo_stmt_errcode($this->stmt);
        // php-src pdo_handle_error builds "SQLSTATE[%s]: %s: %d %s" (state,
        // description, native code, driver message); errorInfo keeps the raw
        // [state, native, message] triple frameworks read via $e->errorInfo.
        $_full = "SQLSTATE[" . $_sqlstate . "]: " . __elephc_pdo_sqlstate_description($_sqlstate) . ": " . $_native . " " . $message;
        if ($this->errMode == 2) {
            throw PDOException::__elephcFromErrorInfo($_full, [$_sqlstate, $_native, $message]);
        }
        fwrite(STDERR, "PDO error: " . $_full . "\n");
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
        $_full = __elephc_pdo_impl_error_message($sqlstate, $message);
        if ($this->errMode == 2) {
            throw PDOException::__elephcFromErrorInfo($_full, [$sqlstate, 0]);
        }
        fwrite(STDERR, "PDO error: " . $_full . "\n");
    }

    public function errorCode(): ?string {
        // The 5-character SQLSTATE for the statement's last operation.
        if (!$this->hasOperation) {
            return null;
        }
        return elephc_pdo_stmt_sqlstate($this->stmt);
    }

    public function errorInfo(): array {
        // Per-statement [SQLSTATE, native, message], mirroring PDO::errorInfo().
        if (!$this->hasOperation) {
            return ["", null, null];
        }
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

    private function copyConstructorArgs(mixed $source): array {
        $_copy = [];
        foreach ($source as $_key => $_value) {
            $_copy[$_key] = $_value;
        }
        return $_copy;
    }

    public function setFetchMode(int $mode, mixed ...$args): bool {
        $_argCount = count($args);
        $classOrColumn = $_argCount > 0 ? $args[0] : null;
        $_constructorArgs = $_argCount > 1 ? $args[1] : null;
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
        // -- elephc PHP >= 8.5 setFetchMode class flags --
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
        if ($_base == 8 && ($mode & 0x40000) != 0 && $_argCount != 0) {
            throw new ValueError("PDOStatement::setFetchMode() expects exactly 1 argument for the fetch mode provided, " . (1 + $_argCount) . " given");
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
        if ($_base == 7 && $_argCount != 1) {
            throw new ValueError("PDOStatement::setFetchMode() expects exactly 2 arguments for the fetch mode provided, 1 given");
        }
        // FETCH_CLASS is the one base mode whose class argument is OPTIONAL — but only
        // under CLASSTYPE, which supplies it from the data instead (and which the gate
        // above has already proven was NOT accompanied by an explicit one).
        if ($_base == 8 && ($mode & 0x40000) == 0 && ($_argCount < 1 || $_argCount > 2)) {
            throw new ValueError("PDOStatement::setFetchMode() expects at least 2 arguments for the fetch mode provided, 1 given");
        }
        if ($_base == 9 && $_argCount != 1) {
            throw new ValueError("PDOStatement::setFetchMode() expects exactly 2 arguments for the fetch mode provided, 1 given");
        }
        if ($_base != 7 && $_base != 8 && $_base != 9 && $_argCount != 0) {
            throw new ValueError("PDOStatement::setFetchMode() expects exactly 1 argument for the fetch mode provided, " . (1 + $_argCount) . " given");
        }
        if ($_base == 8 && $_constructorArgs !== null && !is_array($_constructorArgs)) {
            throw new TypeError("PDOStatement::setFetchMode(): Argument #3 must be of type array, " . $this->argValueTypeName($_constructorArgs) . " given");
        }
        $this->fetchMode = $mode;
        $this->fetchPropsLate = ($mode & 0x100000) != 0;
        $this->fetchCtorArgs = [];
        if ($_base == 7 && $classOrColumn !== null) {
            $this->fetchColumn = (int) $classOrColumn;
        } elseif (($_base == 8 || $_base == 9) && $classOrColumn !== null) {
            $this->fetchTarget = $classOrColumn;
        }
        if ($_base == 8 && is_array($_constructorArgs)) {
            $this->fetchCtorArgs = $this->copyConstructorArgs($_constructorArgs);
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
        } elseif ($this->scrollable) {
            // PostgreSQL's real scrollable-cursor execute path issues FETCH FORWARD 0:
            // execute/materialize the result and leave the cursor before row one.
            $_positioned = elephc_pdo_step_oriented($this->stmt, 4, 0);
            $this->hasPendingStep = false;
            if ($_positioned < 0) {
                $this->fail(elephc_pdo_errmsg($this->conn));
                $this->rowCount = elephc_pdo_changes($this->conn);
                return false;
            }
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

    public function bindParam($parameter, string|int|float|bool|null &$variable, int $type = 2, int $maxLength = 0, mixed $driverOptions = null): bool {
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
        // Capture a getter over the caller's durable reference cell. The ordinary
        // bindValue bookkeeping supplies slot/name/type metadata; execute() replaces
        // its stored snapshot with this getter's current value immediately before bind.
        $_ok = $this->bindValue($parameter, $variable, $type);
        $_boundIndex = count($this->boundValues) - 1;
        $_getter = function() use (&$variable): mixed {
            return $variable;
        };
        $this->boundParamRefIndexes[] = $_boundIndex;
        $this->boundParamRefGetters[] = $_getter;
        // $maxLength (the LOB/output-buffer length hint) and $driverOptions are accepted
        // for signature compatibility with the common
        // `bindParam($p, $v, PDO::PARAM_STR, 4000)` idiom but not applied — the
        // bind loop in execute() has no by-reference length cap or driver-option
        // channel to feed them into.
        $_unusedMaxLength = $maxLength;
        $_unusedDriverOptions = $driverOptions;
        return $_ok;
    }

    public function bindColumn(string|int $column, string|int|float|bool|null &$var, int $type = 2, int $maxLength = 0, mixed $driverOptions = null): bool {
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
        // The PHP subset cannot store `=&` into a property, but its closure
        // environments do own durable reference cells. Capture the destination by
        // reference and retain that setter on the statement; every successful cursor
        // advance invokes it with the freshly converted column value.
        // The direct null-preserving branch also makes the frontend's local-use analysis
        // see the by-reference parameter; closure captures are intentionally not counted
        // by that warning pass yet.
        if (is_null($var)) {
            $var = null;
        }
        $_setter = function(string|int|float|bool|null $_value) use (&$var): void {
            $var = $_value;
        };
        if (is_int($column)) {
            $this->boundColumnKinds[] = 0;
            $this->boundColumnIndexes[] = (int) $column;
            $this->boundColumnNames[] = "";
        } else {
            $this->boundColumnKinds[] = 1;
            $this->boundColumnIndexes[] = 0;
            $this->boundColumnNames[] = (string) $column;
        }
        $this->boundColumnSetters[] = $_setter;
        $this->boundColumnTypes[] = $type;
        $_unusedMaxLength = $maxLength;
        $_unusedDriverOptions = $driverOptions;
        return true;
    }

    public function execute(?array $params = null): bool {
        $this->executed = true;
        $this->hasOperation = true;
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
                $_refCount = count($this->boundParamRefIndexes);
                for ($_ri = 0; $_ri < $_refCount; $_ri++) {
                    if ($this->boundParamRefIndexes[$_ri] == $_i) {
                        $_getter = $this->boundParamRefGetters[$_ri];
                        if (is_callable($_getter)) {
                            callable $_typedGetter = $_getter;
                            $_value = call_user_func_array($_typedGetter, []);
                        }
                        break;
                    }
                }
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
                $_rawBindType = (int) $this->boundTypes[$_i];
                $_btype = $_rawBindType & 0xFFFF;
                if ($_slot < 1) {
                    // bindValue()/bindParam() now reject a positional slot below 1 up
                    // front, so a slot of 0 reaching here can only be a NAMED
                    // placeholder that bind_parameter_index() could not resolve —
                    // php-src's "parameter was not defined" flavor of HY093.
                    $_bindError = "parameter was not defined";
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
                    $_stringFlags = $_rawBindType & 0x60000000;
                    $_national = $_stringFlags == 0x40000000 || ($_stringFlags == 0 && $this->defaultStrParam == 0x40000000);
                    if ($_national) {
                        $_brc = elephc_pdo_bind_text_national($this->stmt, $_slot, $_s, strlen($_s));
                    } else {
                        $_brc = elephc_pdo_bind_text($this->stmt, $_slot, $_s, strlen($_s));
                    }
                }
                if ($_brc == 0) {
                    // The slot resolved but the driver refused it: an out-of-range
                    // positional index (e.g. bindValue(5, ...) on a 2-placeholder
                    // statement), which is php-src's bare "Invalid parameter number".
                    $_bindError = "__elephc_pdo_no_detail";
                    break;
                }
                $this->boundNormalizedIndexes[] = $_i;
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
            $this->boundNormalizedIndexes = [];
            $this->boundParamRefIndexes = [];
            $this->boundParamRefGetters = [];
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
                    $_bindError = "parameter was not defined";
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
                    if ($this->defaultStrParam == 0x40000000) {
                        $_prc = elephc_pdo_bind_text_national($this->stmt, $_pslot, $_ps, strlen($_ps));
                    } else {
                        $_prc = elephc_pdo_bind_text($this->stmt, $_pslot, $_ps, strlen($_ps));
                    }
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
                    $_bindError = "__elephc_pdo_no_detail";
                    break;
                }
                $this->boundNormalizedIndexes[] = count($this->boundValues) - 1;
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
            $_bindDetail = $_bindError === "__elephc_pdo_no_detail" ? "" : $_bindError;
            $this->failCode("HY093", $_bindDetail);
            return false;
        }
        // A statement with no result columns (INSERT/UPDATE/DELETE/DDL) is run
        // now.
        if (elephc_pdo_column_count($this->stmt) == 0) {
            $_step = elephc_pdo_step($this->stmt);
            if ($this->owner !== null && elephc_pdo_driver_name($this->conn) === "pgsql") {
                $this->owner->__elephcDrainPgsqlNotices();
            }
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
            if ($this->owner !== null && elephc_pdo_driver_name($this->conn) === "pgsql") {
                $this->owner->__elephcDrainPgsqlNotices();
            }
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
        $_stringifyFetches = $this->currentStringifyFetches();
        $_oracleNulls = $this->currentOracleNulls();
        if ($_type == 1) {
            $_intVal = elephc_pdo_column_int($this->stmt, $index);
            if (elephc_pdo_driver_name($this->conn) === "pgsql"
                && elephc_pdo_column_native_type($this->stmt, $index) === "bool") {
                return $_intVal != 0;
            }
            if ($_stringifyFetches) {
                return (string) $_intVal;
            }
            return $_intVal;
        } elseif ($_type == 2) {
            $_dblVal = elephc_pdo_column_double($this->stmt, $index);
            if ($_stringifyFetches) {
                return (string) $_dblVal;
            }
            return $_dblVal;
        } elseif ($_type == 5) {
            // NULL is never stringified, matching PHP. P2-e: ATTR_ORACLE_NULLS's
            // NULL_TO_STRING (2) converts it to "" here, mirroring php-src's
            // fetch_value() (its final `oracle_nulls == PDO_NULL_TO_STRING` check).
            if ($_oracleNulls == 2) {
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
        if ($_oracleNulls == 1 && $_out === "") {
            return null;
        }
        if ($_type == 4 && elephc_pdo_driver_name($this->conn) === "pgsql") {
            $_stream = fopen("php://memory", "r+");
            fwrite($_stream, $_out);
            rewind($_stream);
            return $_stream;
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
        $_attrCase = $this->currentAttrCase();
        if ($_attrCase == 1) {
            return strtoupper($_raw);
        }
        if ($_attrCase == 2) {
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

    private function hydrateClass(string $class, int $start, int $count): mixed {
        if ($this->fetchPropsLate) {
            return $this->assignColumnsFrom(new $class(...$this->fetchCtorArgs), $start, $count);
        }
        $_object = $this->assignColumnsFrom(__elephc_new_without_constructor($class), $start, $count);
        if (__elephc_class_has_constructor($class)) {
            call_user_func_array([$_object, "__construct"], $this->fetchCtorArgs);
        } elseif (count($this->fetchCtorArgs) != 0) {
            throw new Error("Class " . $class . " does not have a constructor, so you cannot pass any constructor arguments");
        }
        return $_object;
    }

    // Applies PDO's output-column bindings after a successful cursor advance. Named
    // bindings use the post-ATTR_CASE column names, exactly like php-src's column
    // description table. A missing name remains inert until a later execution exposes it.
    private function updateBoundColumns(): void {
        $_columnCount = elephc_pdo_column_count($this->stmt);
        $_bindingCount = count($this->boundColumnSetters);
        for ($_bi = 0; $_bi < $_bindingCount; $_bi++) {
            // PDO keeps only the last registration for a column key. Registrations are
            // append-only here so descriptor ownership stays simple; skip any entry that
            // has an identical key later in the arrays.
            $_shadowed = false;
            for ($_bj = $_bi + 1; $_bj < $_bindingCount; $_bj++) {
                if ($this->boundColumnKinds[$_bj] == $this->boundColumnKinds[$_bi]
                    && $this->boundColumnIndexes[$_bj] == $this->boundColumnIndexes[$_bi]
                    && $this->boundColumnNames[$_bj] === $this->boundColumnNames[$_bi]) {
                    $_shadowed = true;
                    break;
                }
            }
            if ($_shadowed) {
                continue;
            }

            $_columnIndex = -1;
            if ($this->boundColumnKinds[$_bi] == 0) {
                $_columnIndex = ((int) $this->boundColumnIndexes[$_bi]) - 1;
            } else {
                $_key = $this->boundColumnNames[$_bi];
                for ($_ci = 0; $_ci < $_columnCount; $_ci++) {
                    if ($this->columnName($_ci) === $_key) {
                        $_columnIndex = $_ci;
                        break;
                    }
                }
            }
            if ($_columnIndex < 0 || $_columnIndex >= $_columnCount) {
                continue;
            }

            $_value = $this->columnValue($_columnIndex);
            $_type = ((int) $this->boundColumnTypes[$_bi]) & 0xFFFF;
            if ($_value !== null) {
                if ($_type == 0) {
                    $_value = null;
                } elseif ($_type == 1) {
                    $_value = (int) $_value;
                } elseif ($_type == 2) {
                    $_value = (string) $_value;
                } elseif ($_type == 5) {
                    $_value = (bool) $_value;
                }
            }
            $_setter = $this->boundColumnSetters[$_bi];
            if (is_callable($_setter)) {
                callable $_typedSetter = $_setter;
                call_user_func_array($_typedSetter, [$_value]);
            }
        }
    }

    // Advances the cursor and returns elephc_pdo_step()'s result code
    // (negative = error, 0 = no more rows, positive = a row is available).
    // Every caller that consumes rows from this statement's cursor (fetch(),
    // fetchColumn(), fetchObject(), and fetchAll()'s FETCH_KEY_PAIR loop) goes
    // through this instead of calling elephc_pdo_step() directly, so that
    // execute()'s eager pre-step (see execute()'s comment; P1-4) is consumed
    // exactly once instead of being silently skipped past.
    private function stepCursor(int $orientation = 0, int $offset = 0): int {
        if ($this->scrollable) {
            $_rc = elephc_pdo_step_oriented($this->stmt, $orientation, $offset);
            if ($_rc > 0) {
                $this->updateBoundColumns();
            }
            return $_rc;
        }
        if ($this->hasPendingStep) {
            $this->hasPendingStep = false;
            $_rc = $this->pendingStep;
        } else {
            $_rc = elephc_pdo_step($this->stmt);
        }
        if ($_rc > 0) {
            $this->updateBoundColumns();
        }
        return $_rc;
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
    // Forward-only SQLite/MySQL statements ignore orientation like php-src. PostgreSQL
    // scroll cursors honor all FETCH_ORI_* values and the ABS/REL offset.
    public function fetch(int $mode = 0, int $cursorOrientation = 0, int $cursorOffset = 0): mixed {
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
        // FETCH_LAZY is valid for fetch() and returns the statement's one reusable
        // PDORow object. fetchAll() rejects it separately, matching php-src.
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
        // stepCursor() performs bindColumn() write-back before this branch sees
        // the successful result, so FETCH_BOUND itself only returns the row status.
        if ($_base == 6) {
            $_boundRc = $this->stepCursor($cursorOrientation, $cursorOffset);
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
        $_rc = $this->stepCursor($cursorOrientation, $cursorOffset);
        if ($_rc < 0) {
            $this->fail(elephc_pdo_errmsg($this->conn));
            return false;
        }
        if ($_rc == 0) {
            return false;
        }
        $_count = elephc_pdo_column_count($this->stmt);
        if ($_base == 1) {
            $_lazyValues = [];
            $_lazyNames = [];
            for ($_li = 0; $_li < $_count; $_li++) {
                $_lazyValues[] = $this->columnValue($_li);
                $_lazyNames[] = $this->columnName($_li);
            }
            if (!($this->lazyRow instanceof PDORow)) {
                $this->lazyRow = new PDORow(true, $this->queryString);
            }
            $_lazyRow = $this->lazyRow;
            if ($_lazyRow instanceof PDORow) {
                PDORow $_typedLazyRow = $_lazyRow;
                $_typedLazyRow->__elephcRefresh($_lazyValues, $_lazyNames);
                return $_typedLazyRow;
            }
            return false;
        }
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
                return $this->hydrateClassOrStd($_ctName, 1, $_count);
            }
            if ($this->fetchTarget !== null) {
                $_classTarget = $this->fetchTarget;
                return $this->hydrateClass($_classTarget, 0, $_count);
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

    public function fetchAll(int $mode = 0, mixed ...$args): array {
        $_fetchAllArgCount = count($args);
        $classOrObject = $_fetchAllArgCount > 0 ? $args[0] : null;
        $ctorArgs = $_fetchAllArgCount > 1 ? $args[1] : null;
        //
        // NOTE that fetchAll() KEEPS its `mixed $classOrObject` second parameter while
        // fetch() (F-STMT-01) loses its own: that is not an inconsistency, it is php-src.
        // fetchAll's stub really does take the fetch-mode's extra arguments
        // (`fetchAll(int $mode = PDO::FETCH_DEFAULT, mixed ...$args)`); fetch's really
        // does not (its 2nd parameter is `int $cursorOrientation`). The two methods
        // diverge in php-src exactly as they now diverge here.
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
            // FETCH_FUNC calls the supplied callback with one positional argument per
            // column and collects its return values. The public variadic PHP signature is
            // represented by this prelude's bounded extra slots, so argument #2 carries
            // the callback. A divergent is_callable() guard narrows the Mixed slot to a
            // callable; EIR then dispatches the narrowed Mixed value through the same
            // descriptor/name selector used by call_user_func_array() elsewhere.
            if (!is_callable($classOrObject)) {
                throw new TypeError("PDOStatement::fetchAll(): Argument #2 must be a valid callback");
            }
            $_fetchFunc = $classOrObject;
            if (!$this->executed) {
                return [];
            }
            $_funcRows = [];
            while (true) {
                $_frc = $this->stepCursor();
                if ($_frc < 0) {
                    $this->fail(elephc_pdo_errmsg($this->conn));
                    break;
                }
                if ($_frc == 0) {
                    break;
                }
                $_funcArgs = [];
                $_funcCount = elephc_pdo_column_count($this->stmt);
                for ($_fi = 0; $_fi < $_funcCount; $_fi++) {
                    $_funcArgs[] = $this->columnValue($_fi);
                }
                $_funcRows[] = call_user_func_array($_fetchFunc, $_funcArgs);
            }
            return $_funcRows;
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
        if ($_base == 8) {
            if ($_fetchAllArgCount > 2) {
                throw new ValueError("PDOStatement::fetchAll() expects at most 3 arguments for the fetch mode provided, " . (1 + $_fetchAllArgCount) . " given");
            }
            if ($ctorArgs !== null && !is_array($ctorArgs)) {
                throw new TypeError("PDOStatement::fetchAll(): Argument #3 must be of type array, " . $this->argValueTypeName($ctorArgs) . " given");
            }
            if (is_array($ctorArgs)) {
                $this->fetchCtorArgs = $this->copyConstructorArgs($ctorArgs);
            } else {
                $this->fetchCtorArgs = [];
            }
            $this->fetchPropsLate = ($mode & 0x100000) != 0;
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
            // The key is CAST TO STRING first, exactly as php-src does
            // (`convert_to_string`). groupKey() then applies PHP's array-key conversion:
            // a canonical base-10 integer string that round-trips through int becomes an
            // integer key, while leading-zero, plus-prefixed, overflow, and "-0" spellings
            // remain strings.
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
            // EXISTENCE IS TESTED BY A count() PROBE. isset()/array_key_exists() now work
            // for these key shapes in isolation, but rewriting this mixed-row loop to a
            // direct nested append reintroduces an array-representation mismatch at the
            // control-flow join. The presence-map count delta avoids that compiler edge:
            // storing a known key does not grow the map, while a new key grows it once.
            // It is sound for any key value, null included, and remains O(n).
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
                    // Append to the existing bucket. Detach it from the map FIRST: after the
                    // read the bucket has refcount 2 (the $_groups slot + $_bucket), so a bare
                    // push would COW-clone the whole bucket every row — O(n^2) over a large
                    // group (n = rows in the group). unset() drops the slot's reference to
                    // refcount 1, so `$_bucket[] = …` mutates in place (amortized O(1)); the
                    // same bucket is then reinserted under the same key. $_out is assembled
                    // from $_order (first-seen), never from $_groups iteration order, so the
                    // detach/reinsert cannot change the output; $_present is untouched, so the
                    // key stays "seen". This is the O(n^2)->O(n) fix, with no compiler change.
                    $_bucket = $_groups[$_gkeyS];
                    unset($_groups[$_gkeyS]);
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

    private function hydrateClassOrStd(string $class, int $start, int $count): mixed {
        if ($this->fetchPropsLate) {
            return $this->assignColumnsFromOrStd(new $class(...$this->fetchCtorArgs), $start, $count);
        }
        $_object = __elephc_new_without_constructor($class);
        if ($_object === null) {
            return $this->assignColumnsFrom(new stdClass(), $start, $count);
        }
        $_object = $this->assignColumnsFrom($_object, $start, $count);
        if (__elephc_class_has_constructor($class)) {
            call_user_func_array([$_object, "__construct"], $this->fetchCtorArgs);
        }
        return $_object;
    }

    // F-STMT-15: the FETCH_GROUP / FETCH_UNIQUE grouping key, taken from column 0 and CAST
    // TO STRING exactly as php-src does (pdo_stmt.c do_fetch: `convert_to_string(&grp_val)`
    // before the key ever reaches the hash table).
    //
    // Declared `: mixed` DELIBERATELY, not `: string` — see the call site. Returning the
    // key as Mixed is what keeps fetchAll()'s $_out an Array(Mixed) instead of promoting it
    // to a statically-typed AssocArray it could then not return through `: array`.
    private function groupKey(int $index): mixed {
        $_key = (string) $this->columnValue($index);
        $_integerKey = (int) $_key;
        if ((string) $_integerKey === $_key) {
            return $_integerKey;
        }
        return $_key;
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
                return $this->hydrateClass($_gClass, 1, $count);
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
        $this->fetchCtorArgs = $this->copyConstructorArgs($constructorArgs);
        $this->fetchPropsLate = false;
        return $this->hydrateClass((string) $class, 0, $_count);
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
        if ($name == 1001 && elephc_pdo_driver_name($this->conn) === "pgsql") {
            $this->hasOperation = true;
            $_memory = elephc_pdo_result_memory_size($this->stmt);
            return $_memory < 0 ? null : $_memory;
        }
        // P2-16: Pdo\Sqlite::ATTR_READONLY_STATEMENT is a LIVE sqlite3_stmt_readonly()
        // read rather than a stored value — it reflects the actual prepared
        // statement, not a value the caller set. The bridge reports 0 for a
        // non-SQLite statement, which reads back as false there too.
        if ($name == 1001) {
            return elephc_pdo_stmt_readonly($this->stmt) === 1;
        }
        if ($name == 1003 && elephc_pdo_driver_name($this->conn) === "sqlite") {
            return elephc_pdo_stmt_busy($this->stmt) === 1;
        }
        if ($name == 1004 && elephc_pdo_driver_name($this->conn) === "sqlite") {
            return elephc_pdo_stmt_explain_mode($this->stmt);
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
        if ($attribute == 1004 && elephc_pdo_driver_name($this->conn) === "sqlite") {
            if (!is_int($value)) {
                throw new TypeError("explain mode must be of type int, " . $this->argValueTypeName($value) . " given");
            }
            $_explainMode = (int) $value;
            if ($_explainMode < 0 || $_explainMode > 2) {
                throw new ValueError("explain mode must be one of the Pdo\\Sqlite::EXPLAIN_MODE_* constants");
            }
            return elephc_pdo_stmt_set_explain_mode($this->stmt, $_explainMode) === 1;
        }
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
        // MySQL retains every protocol result set during execute(), including
        // empty OK-packet sets between SELECT-like sets. Advancing resets the
        // row cursor and refreshes rowCount()/column metadata for the new set.
        if (elephc_pdo_driver_name($this->conn) === "mysql") {
            if (elephc_pdo_next_rowset($this->stmt) !== 1) {
                return false;
            }
            $this->hasPendingStep = false;
            $this->pendingStep = 0;
            $this->executed = true;
            $this->hasOperation = true;
            $this->rowCount = elephc_pdo_changes($this->conn);
            return true;
        }
        $this->failCode("IM001", "driver does not support multiple rowsets");
        return false;
    }

    public function getColumnMeta(int $column): array|bool {
        // PDOStatement::getColumnMeta is assembled from the common PDO descriptor and
        // the active driver's metadata. Returns false for an out-of-range column index.
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
        // and now `len` (PQfsize), `precision` (PQfmod), `pgsql:table_oid` (PQftable),
        // and the source table name resolved through `pg_class` when one exists.
        //
        // A `mysql:` statement gets OID 0 and is handled by the explicit MySQL branch
        // below. SQLite then falls through to its runtime-storage-class metadata.
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
            $_pgMeta = [
                "name" => $this->columnName($column),
                "native_type" => elephc_pdo_column_native_type($this->stmt, $column),
                "pdo_type" => $_pgType,
                "len" => elephc_pdo_column_len($this->stmt, $column),
                "precision" => elephc_pdo_column_precision($this->stmt, $column),
                "flags" => [],
                "pgsql:oid" => $_oid,
                "pgsql:table_oid" => elephc_pdo_column_table_oid($this->stmt, $column),
            ];
            $_pgTable = elephc_pdo_column_table_name($this->stmt, $column);
            if ($_pgTable !== "") {
                $_pgMeta["table"] = $_pgTable;
            }
            return $_pgMeta;
        }
        $_driver = elephc_pdo_driver_name($this->conn);
        if ($_driver === "mysql") {
            $_myNative = elephc_pdo_column_native_type($this->stmt, $column);
            $_myType = 2;
            if ($_myNative === "BIT" || $_myNative === "YEAR" || $_myNative === "TINY"
                || $_myNative === "SHORT" || $_myNative === "INT24" || $_myNative === "LONG"
                || $_myNative === "LONGLONG") {
                $_myType = 1;
            }
            $_myFlags = [];
            $_myFlagBits = elephc_pdo_column_flags($this->stmt, $column);
            if (($_myFlagBits & 1) !== 0) {
                $_myFlags[] = "not_null";
            }
            if (($_myFlagBits & 2) !== 0) {
                $_myFlags[] = "primary_key";
            }
            if (($_myFlagBits & 8) !== 0) {
                $_myFlags[] = "multiple_key";
            }
            if (($_myFlagBits & 4) !== 0) {
                $_myFlags[] = "unique_key";
            }
            if (($_myFlagBits & 16) !== 0) {
                $_myFlags[] = "blob";
            }
            // mysqlnd omits native_type for an unknown wire type rather than inventing
            // a storage-class fallback. The binary column packet carries no default
            // value, so `mysql:def` is likewise omitted when unavailable.
            if ($_myNative === "") {
                return [
                    "pdo_type" => $_myType,
                    "flags" => $_myFlags,
                    "table" => elephc_pdo_column_table_name($this->stmt, $column),
                    "name" => $this->columnName($column),
                    "len" => elephc_pdo_column_len($this->stmt, $column),
                    "precision" => elephc_pdo_column_precision($this->stmt, $column),
                ];
            }
            return [
                "native_type" => $_myNative,
                "pdo_type" => $_myType,
                "flags" => $_myFlags,
                "table" => elephc_pdo_column_table_name($this->stmt, $column),
                "name" => $this->columnName($column),
                "len" => elephc_pdo_column_len($this->stmt, $column),
                "precision" => elephc_pdo_column_precision($this->stmt, $column),
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
        $_meta = [
            "name" => $this->columnName($column),
            "native_type" => $_native,
            "pdo_type" => $_pdoType,
            "len" => -1,
            "precision" => 0,
            "flags" => $_flags,
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
        $_table = elephc_pdo_column_table_name($this->stmt, $column);
        if ($_table !== "") {
            $_meta["table"] = $_table;
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
        // The arrays are append-only for reference ownership, but php-src stores a hash:
        // only the last bind for a positional slot or named placeholder is visible here.
        // Named parameters retain paramno=-1 until execute-time normalization.
        echo "SQL: [" . strlen($this->queryString) . "] " . $this->queryString . "\n";
        $_sentSql = elephc_pdo_stmt_sent_sql($this->stmt);
        if ($_sentSql !== "") {
            echo "Sent SQL: [" . strlen($_sentSql) . "] " . $_sentSql . "\n";
        }
        $_recordCount = count($this->boundValues);
        $_pcount = 0;
        for ($_i = 0; $_i < $_recordCount; $_i++) {
            $_shadowed = false;
            for ($_j = $_i + 1; $_j < $_recordCount; $_j++) {
                $_bothPositional = $this->boundNames[$_i] === "" && $this->boundNames[$_j] === "";
                $_bothNamed = $this->boundNames[$_i] !== "" && $this->boundNames[$_j] !== "";
                if (($_bothPositional || $_bothNamed) && $this->boundParams[$_i] == $this->boundParams[$_j]) {
                    $_shadowed = true;
                    break;
                }
            }
            if (!$_shadowed) {
                $_pcount = $_pcount + 1;
            }
        }
        echo "Params:  " . $_pcount . "\n";
        for ($_i = 0; $_i < $_recordCount; $_i++) {
            $_shadowed = false;
            for ($_j = $_i + 1; $_j < $_recordCount; $_j++) {
                $_bothPositional = $this->boundNames[$_i] === "" && $this->boundNames[$_j] === "";
                $_bothNamed = $this->boundNames[$_i] !== "" && $this->boundNames[$_j] !== "";
                if (($_bothPositional || $_bothNamed) && $this->boundParams[$_i] == $this->boundParams[$_j]) {
                    $_shadowed = true;
                    break;
                }
            }
            if ($_shadowed) {
                continue;
            }
            $_dname = (string) $this->boundNames[$_i];
            // php's paramno is 0-based; the recorded slot is the driver's 1-based index.
            $_dno = ((int) $this->boundParams[$_i]) - 1;
            if ($_dname !== "") {
                $_normalized = false;
                foreach ($this->boundNormalizedIndexes as $_normalizedIndex) {
                    if ($_normalizedIndex == $_i) {
                        $_normalized = true;
                        break;
                    }
                }
                if (!$_normalized) {
                    $_dno = -1;
                }
            }
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

    public function getIterator(): \Iterator {
        return new __ElephcPDOStatementIterator($this);
    }

    public function __destruct() {
        // Finalize the prepared statement when the PDOStatement is collected. The
        // bridge ignores an unknown/already-finalized handle, so this is safe even
        // when the owning PDO connection was closed first (its close() already
        // finalized this statement).
        elephc_pdo_finalize($this->stmt);
        // Drop the explicit connection root while the object is still fully
        // initialized. This makes the final PDO owner observable immediately and
        // avoids relying on post-destructor property sweeping for a nullable object
        // slot whose runtime representation is boxed Mixed.
        $this->owner = null;
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

/// Prefixed userland adapter for php-src's internal PDO statement iterator.
final class __ElephcPDOStatementIterator implements Iterator {
    private PDOStatement $statement;
    private mixed $row;
    private int $position;

    public function __construct(PDOStatement $statement) {
        $this->statement = $statement;
        $this->row = null;
        $this->position = 0;
    }

    public function rewind(): void {
        $this->row = $this->statement->fetch();
        $this->position = 0;
    }

    public function valid(): bool {
        return $this->row !== false;
    }

    public function current(): mixed {
        return $this->row;
    }

    public function key(): mixed {
        return $this->position;
    }

    public function next(): void {
        $this->row = $this->statement->fetch();
        $this->position = $this->position + 1;
    }
}

// PHP 8.4 driver-specific PDO subclasses. They are returned by the DSN-dispatching
// `PDO::connect()` factory (defined above) and can also be constructed directly;
// each inherits the full base PDO connection surface (constructor, exec/query/
// prepare, transactions, quoting) from \PDO, and adds its driver-specific
// constants and driver methods. Callback methods use rooted callable descriptors
// and shared C-to-PHP adapters; connection-backed methods delegate to the PDO bridge.
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
// -- elephc PHP >= 8.4 namespaced PDO drivers begin --
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
        // 8.5-READINESS: prelude_source_for_version() inserts the busy/explain/
        // transaction attributes, mode/authorizer constants, and setAuthorizer()
        // only for PHP 8.5 and later. The default PHP 8.4 source intentionally ends
        // at ATTR_EXTENDED_RESULT_CODES.

        // Roots the collation / user-function callbacks registered on this
        // connection. SQLite keeps a raw C pointer to each callback's compiled-PHP
        // descriptor for the connection's lifetime, so the descriptor must stay
        // reachable from PHP; this array is that GC root.
        // Dedicated authorizer root. This is deliberately untyped and seeded with
        // a closure: elephc then gives the property callable storage, whose assignment
        // path retains replacement closure descriptors. A Mixed property or an array
        // element only retains the boxed container today, allowing the descriptor
        // backing a replaced callback to be recycled before SQLite calls it.
        private $authorizerCallback;

        public function __construct(string $dsn, ?string $username = null, #[\SensitiveParameter] ?string $password = null, ?array $options = null) {
            // F-CORE-01/F-CORE-11: resolve an indirect `uri:` DSN FIRST (php-src resolves
            // it before it compares the DSN's driver against the called scope), then reject
            // a DSN belonging to another driver BEFORE any connection is attempted. The
            // resolved DSN is what goes up to \PDO, so the file is read exactly once —
            // resolveDsnUri() is a no-op on an already-resolved DSN.
            $_operation = get_class($this) . "::__construct";
            $_sqliteDsn = self::resolveDsnAlias($dsn, $_operation);
            $_sqliteDsn = self::resolveDsnUri($_sqliteDsn, $_operation);
            $this->checkDriverSubclassDsn($_sqliteDsn, "Pdo\\Sqlite", "sqlite");
            // Forward to \PDO to open the connection, then initialise the callback
            // root (an uninitialised typed array property is not implicitly []).
            parent::__construct($_sqliteDsn, $username, $password, $options);
            $this->authorizerCallback = function() { return 0; };
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
            // The compiler-owned wrapper keeps an independent seek cursor and applies
            // every writable patch immediately through sqlite3_blob_write. SQLite's
            // incremental BLOB contract fixes the size at open time, so an extending
            // write fails while reads, seeks, embedded NULs, and in-place writes match
            // the native PDO stream.
            $_db = ($dbname === null) ? "main" : $dbname;
            return \__ElephcPDOSqliteBlobStream::create($this->connectionId(), $table, $column, $rowid, $_db, $flags);
        }

        public function createCollation(string $name, mixed $callback): bool {
            // Registers a custom collation `$name` backed by a compiled-PHP
            // comparator `$callback($a, $b): int` (returning <0, 0, >0). The callable
            // is decomposed here into its descriptor pointer and the shared codegen
            // collation adapter address, so the bridge extern receives two plain
            // `ptr` args and never a `callable`. The callback is rooted in
            // $this->udfCallbacks first because SQLite keeps a C pointer to its
            // descriptor for the connection's lifetime. The key is namespaced so a
            // same-named collation and scalar function do not evict each other's GC
            // root. __elephc_normalize_callable converts every supported PHP callable
            // form to the descriptor representation consumed by the adapter.
            if (!\is_callable($callback)) {
                throw new \TypeError("Pdo\\Sqlite::createCollation(): Argument #2 (\$callback) must be a valid callback");
            }
            $_normalized = \__elephc_normalize_callable($callback);
            $_descriptor = \__elephc_callable_ptr($_normalized);
            $_adapter = \__elephc_pdo_adapter_addr(0);
            if (\elephc_pdo_create_collation($this->connectionId(), $name, $_descriptor, $_adapter) !== 1) {
                return false;
            }
            $this->pdoUdfCallbacks["collation:" . \strtolower($name)] = $_normalized;
            return true;
        }

        public function createFunction(string $function_name, mixed $callback, int $num_args = -1, int $flags = 0): bool {
            // Registers a scalar SQL function `$function_name` backed by a compiled-PHP
            // `$callback(...$args): mixed` invoked once per row. Like createCollation,
            // the callable is decomposed here into its descriptor pointer and the shared
            // codegen scalar adapter address, so the bridge extern receives two plain
            // `ptr` args and never a `callable`. The callback is rooted in
            // $this->udfCallbacks (under a function-namespaced key) first because SQLite
            // keeps a C pointer to its descriptor for the connection's lifetime.
            // $num_args is the declared arity (-1 = variadic); $flags carries
            // self::DETERMINISTIC. Callable normalization accepts closures, names,
            // callable arrays, invokable objects, and first-class descriptors.
            // Parameter names match the PHP stub
            // (`createFunction(string $function_name, callable $callback, int $num_args = -1, int $flags = 0)`)
            // so named-argument calls resolve; the extern call below uses positions,
            // so the rename is otherwise behavior-neutral.
            if (!\is_callable($callback)) {
                throw new \TypeError("Pdo\\Sqlite::createFunction(): Argument #2 (\$callback) must be a valid callback");
            }
            $_normalized = \__elephc_normalize_callable($callback);
            $_descriptor = \__elephc_callable_ptr($_normalized);
            $_adapter = \__elephc_pdo_adapter_addr(1);
            if (\elephc_pdo_create_function($this->connectionId(), $function_name, $num_args, $flags, $_descriptor, $_adapter) !== 1) {
                return false;
            }
            $this->pdoUdfCallbacks["function:" . \strtolower($function_name) . ":" . $num_args . ":scalar"] = $_normalized;
            return true;
        }

        public function createAggregate(string $name, mixed $step, mixed $finalize, int $numArgs = -1): bool {
            // Registers an aggregate SQL function `$name` backed by a compiled-PHP
            // step + finalize pair: `$step($context, $rownumber, ...$values): mixed`
            // runs once per row (returning the new accumulator, null-seeded on the
            // first row) and `$finalize($context, $rownumber): mixed` produces the
            // group result. Each callable is decomposed into its descriptor pointer
            // and the shared codegen adapter address (kinds 2 and 3), so the bridge
            // extern receives four plain `ptr` args and never a `callable`. Both
            // callables are rooted in $this->udfCallbacks (under distinct keys so
            // neither evicts the other's GC root) because SQLite keeps a C pointer to
            // each descriptor for the connection's lifetime. Both callables pass
            // through the same complete normalization path as scalar functions.
            if (!\is_callable($step) || !\is_callable($finalize)) {
                throw new \TypeError("Pdo\\Sqlite::createAggregate(): step and finalize must be valid callbacks");
            }
            $_normalizedStep = \__elephc_normalize_callable($step);
            $_normalizedFinal = \__elephc_normalize_callable($finalize);
            $_stepDesc = \__elephc_callable_ptr($_normalizedStep);
            $_stepAdapter = \__elephc_pdo_adapter_addr(2);
            $_finalDesc = \__elephc_callable_ptr($_normalizedFinal);
            $_finalAdapter = \__elephc_pdo_adapter_addr(3);
            if (\elephc_pdo_create_aggregate($this->connectionId(), $name, $numArgs, $_stepDesc, $_stepAdapter, $_finalDesc, $_finalAdapter) !== 1) {
                return false;
            }
            $_rootKey = "function:" . \strtolower($name) . ":" . $numArgs;
            $this->pdoUdfCallbacks[$_rootKey . ":step"] = $_normalizedStep;
            $this->pdoUdfCallbacks[$_rootKey . ":final"] = $_normalizedFinal;
            return true;
        }

        // -- elephc PHP >= 8.5 SQLite setAuthorizer insertion --
    }

    class Mysql extends \PDO {
        // MySQL/MariaDB driver-specific attribute constants (ext/pdo_mysql, mysqlnd
        // build — the PHP default). Values start at PDO_ATTR_DRIVER_SPECIFIC (1000).
        // The libmysqlclient-only ATTR_MAX_BUFFER_SIZE / ATTR_READ_DEFAULT_* are
        // intentionally omitted (absent under mysqlnd, and their presence would shift
        // every value from ATTR_COMPRESS upward).
        const ATTR_USE_BUFFERED_QUERY = 1000;
        const ATTR_LOCAL_INFILE = 1001;
        // P1-9: honored by PDO::__construct's constructor-options
        // loop, which threads the raw SQL string through to the bridge's connect
        // path (my.rs::MyConn::open -> OptsBuilder::init). The other options below
        // are routed individually or rejected explicitly when the Rust client has
        // no equivalent security control.
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

        public function __construct(string $dsn, ?string $username = null, #[\SensitiveParameter] ?string $password = null, ?array $options = null) {
            // F-CORE-01: this class had NO constructor at all, so `new Pdo\Mysql("sqlite:…")`
            // inherited \PDO's and cheerfully opened a SQLite database behind a Pdo\Mysql
            // object. The override exists solely to run the driver guard (and the `uri:`
            // resolution it depends on) before any connection is attempted; it adds no
            // MySQL-specific state of its own. See \PDO::checkDriverSubclassDsn().
            $_operation = get_class($this) . "::__construct";
            $_mysqlDsn = self::resolveDsnAlias($dsn, $_operation);
            $_mysqlDsn = self::resolveDsnUri($_mysqlDsn, $_operation);
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

        // Connection-owned callback root. A concrete closure (rather than a
        // nullable callable property) keeps compiled callable dispatch precise.
        private $noticeCallback;

        public function __construct(string $dsn, ?string $username = null, #[\SensitiveParameter] ?string $password = null, ?array $options = null) {
            // F-CORE-01/F-CORE-11: resolve an indirect `uri:` DSN, then reject a DSN
            // belonging to another driver, both BEFORE any connection is attempted — see
            // \PDO::checkDriverSubclassDsn().
            $_operation = get_class($this) . "::__construct";
            $_pgsqlDsn = self::resolveDsnAlias($dsn, $_operation);
            $_pgsqlDsn = self::resolveDsnUri($_pgsqlDsn, $_operation);
            $this->checkDriverSubclassDsn($_pgsqlDsn, "Pdo\\Pgsql", "pgsql");
            // Forward to \PDO to open the connection. The base connection object owns
            // a virtual drain hook so prepared statements can dispatch here too.
            parent::__construct($_pgsqlDsn, $username, $password, $options);
            $this->noticeCallback = function($_message) {};
        }

        public function setNoticeCallback(?callable $callback): void {
            // Registers a callback invoked with the text of each PostgreSQL server
            // NOTICE. Passing null unregisters delivery by restoring the no-op callback.
            // The Pdo\Pgsql object owns the callback; the base class only declares
            // the virtual hook used by PDOStatement's PDO-typed owner reference.
            if ($callback === null) {
                $this->noticeCallback = function($_message) {};
                return;
            }
            $this->noticeCallback = $callback;
        }

        protected function __elephcDrainPgsqlNotices(): void {
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
            $this->__elephcDrainPgsqlNotices();
            return $_result;
        }

        public function query(string $query, ?int $fetchMode = null, mixed ...$fetchModeArgs): \PDOStatement|bool {
            // As exec(), but for a row-returning statement. `\PDOStatement` is
            // fully-qualified because this override lives inside `namespace Pdo`, where
            // a bare `PDOStatement` would resolve to the non-existent `Pdo\PDOStatement`.
            // Signature mirrors the widened base PDO::query() (P0-6) so overriding
            // stays arity-compatible; the extra args are simply forwarded.
            $_result = parent::query($query, $fetchMode, ...$fetchModeArgs);
            $this->__elephcDrainPgsqlNotices();
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
            // or false on error. libpq's large-object API requires an explicit
            // transaction, which is enforced before the bridge call.
            if (!$this->inTransaction()) {
                return false;
            }
            $_oid = \elephc_pdo_lob_create($this->connectionId());
            return $_oid === "" ? false : $_oid;
        }

        public function lobUnlink(string $oid): bool {
            // Deletes the large object with the given OID.
            if (!$this->inTransaction()) {
                return false;
            }
            return \elephc_pdo_lob_unlink($this->connectionId(), $oid) === 1;
        }

        public function lobOpen(string $oid, string $mode = "rb"): mixed {
            // A mode containing `+` or `w` is writable, matching php-src's mode test.
            // The wrapper is seekable, extends with zero-filled gaps, and writes each
            // patch back synchronously while the owning transaction remains active.
            return \__ElephcPDOPgsqlLobStream::create($this, $this->connectionId(), $oid, $mode);
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
            // Polls for a pending LISTEN/NOTIFY notification, or false if none
            // arrived within the timeout.
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
            if ($_raw === "") {
                return false;
            }
            // Split only the two framing tabs. The payload is the untouched remainder,
            // so an arbitrary PostgreSQL NOTIFY payload containing tabs stays byte-exact.
            $_sep1 = (int) \strpos($_raw, "\t");
            $_channel = \substr($_raw, 0, $_sep1);
            $_rest = \substr($_raw, $_sep1 + 1);
            $_sep2 = (int) \strpos($_rest, "\t");
            $_pid = (int) \substr($_rest, 0, $_sep2);
            $_payload = \substr($_rest, $_sep2 + 1);
            if ($fetchMode == 2) {
                return ["message" => $_channel, "pid" => $_pid, "payload" => $_payload];
            }
            return [$_channel, $_pid, $_payload];
        }
    }
}
// -- elephc PHP >= 8.4 namespaced PDO drivers end --
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
    inject_if_used_for_version(program, force, PhpVersion::default())
}

/// Prepends the PDO prelude generated for an explicit PHP compatibility version.
///
/// PHP 8.5 renumbered every high fetch-mode flag into the low byte. Generating the
/// constants and all decoding masks from the same version selection prevents a source
/// program compiled for 8.4 from being interpreted with 8.5 flag semantics.
pub fn inject_if_used_for_version(
    program: Program,
    force: bool,
    php_version: PhpVersion,
) -> Program {
    if !force && !detect::program_uses_pdo(&program) {
        return program;
    }
    let source = prelude_source_for_version(php_version);
    let tokens = crate::lexer::tokenize(source.as_ref()).expect("PDO prelude must tokenize");
    let mut combined = crate::parser::parse(&tokens).expect("PDO prelude must parse");
    combined.extend(program);
    combined
}

/// Returns the PDO prelude source with version-specific fetch constants and decoders.
fn prelude_source_for_version(php_version: PhpVersion) -> Cow<'static, str> {
    if php_version == PhpVersion::Php84 {
        let mut source = PDO_PRELUDE_SRC.to_owned();
        remove_version_block(
            &mut source,
            "        // -- elephc PHP >= 8.5 PDO pgsql simple streaming begin --",
            "        // -- elephc PHP >= 8.5 PDO pgsql simple streaming end --",
        );
        return Cow::Owned(source);
    }

    if php_version < PhpVersion::Php84 {
        let mut source = PDO_PRELUDE_SRC.to_owned();
        remove_version_block(
            &mut source,
            "        // -- elephc PHP >= 8.5 PDO pgsql simple streaming begin --",
            "        // -- elephc PHP >= 8.5 PDO pgsql simple streaming end --",
        );
        remove_version_block(
            &mut source,
            "    // -- elephc PHP >= 8.4 PDO::connect begin --",
            "    // -- elephc PHP >= 8.4 PDO::connect end --",
        );
        remove_version_block(
            &mut source,
            "// -- elephc PHP >= 8.4 namespaced PDO drivers begin --",
            "// -- elephc PHP >= 8.4 namespaced PDO drivers end --",
        );
        if php_version == PhpVersion::Php80 {
            // PDOStatement::$queryString and PDORow::$queryString became public
            // properties in PHP 8.1. Keep private storage for the prelude's own SQL
            // bookkeeping under 8.0 without exposing the later user-facing surface.
            source = source.replace(
                "public readonly string $queryString;",
                "private string $queryString;",
            );
        }
        if php_version < PhpVersion::Php82 {
            source = source.replace("#[\\SensitiveParameter] ", "");
        }
        return Cow::Owned(source);
    }

    let mut source = PDO_PRELUDE_SRC
        .replace("const FETCH_GROUP = 0x10000;", "const FETCH_GROUP = 0x20;")
        .replace("const FETCH_UNIQUE = 0x30000;", "const FETCH_UNIQUE = 0x40;")
        .replace("const FETCH_CLASSTYPE = 0x40000;", "const FETCH_CLASSTYPE = 0x80;")
        .replace("const FETCH_SERIALIZE = 0x80000;", "const FETCH_SERIALIZE = 0x200;")
        .replace("const FETCH_PROPS_LATE = 0x100000;", "const FETCH_PROPS_LATE = 0x100;")
        .replace(
            "        const TRANSACTION_IDLE = 0;",
            "        #[\\Deprecated(\"as it has no effect\")]\n        const TRANSACTION_IDLE = 0;",
        )
        .replace(
            "        const TRANSACTION_ACTIVE = 1;",
            "        #[\\Deprecated(\"as it has no effect\")]\n        const TRANSACTION_ACTIVE = 1;",
        )
        .replace(
            "        const TRANSACTION_INTRANS = 2;",
            "        #[\\Deprecated(\"as it has no effect\")]\n        const TRANSACTION_INTRANS = 2;",
        )
        .replace(
            "        const TRANSACTION_INERROR = 3;",
            "        #[\\Deprecated(\"as it has no effect\")]\n        const TRANSACTION_INERROR = 3;",
        )
        .replace(
            "        const TRANSACTION_UNKNOWN = 4;",
            "        #[\\Deprecated(\"as it has no effect\")]\n        const TRANSACTION_UNKNOWN = 4;",
        )
        .replace(
            "const ATTR_EXTENDED_RESULT_CODES = 1002;\n        // 8.5-READINESS:",
            "const ATTR_EXTENDED_RESULT_CODES = 1002;\n        const ATTR_BUSY_STATEMENT = 1003;\n        const ATTR_EXPLAIN_STATEMENT = 1004;\n        const ATTR_TRANSACTION_MODE = 1005;\n        const TRANSACTION_MODE_DEFERRED = 0;\n        const TRANSACTION_MODE_IMMEDIATE = 1;\n        const TRANSACTION_MODE_EXCLUSIVE = 2;\n        const EXPLAIN_MODE_PREPARED = 0;\n        const EXPLAIN_MODE_EXPLAIN = 1;\n        const EXPLAIN_MODE_EXPLAIN_QUERY_PLAN = 2;\n        const OK = 0;\n        const DENY = 1;\n        const IGNORE = 2;\n        // 8.5-READINESS:",
        )
        .replace("$_base = $mode & 0xFFFF;", "$_base = $mode & 0xF;")
        .replace("($mode & 0x40000) != 0", "($mode & 0x80) != 0")
        .replace("($mode & 0x40000) == 0", "($mode & 0x80) == 0")
        .replace("($mode & 0x100000) != 0", "($mode & 0x100) != 0")
        .replace(
            "elseif (($mode & 0x10000) != 0)",
            "elseif ((($mode & 0x20) != 0) || (($mode & 0x40) != 0))",
        )
        .replace(
            "if (($mode & 0x10000) != 0)",
            "if ((($mode & 0x20) != 0) || (($mode & 0x40) != 0))",
        )
        .replace(
            "$_unique = ($mode & 0x30000) == 0x30000;",
            "$_unique = ($mode & 0x40) != 0;",
        )
        .replace(
            "        // -- elephc PHP >= 8.5 setFetchMode class flags --",
            "        if (($mode & (0x80 | 0x100 | 0x200)) != 0 && $_base != 8) {\n            throw new ValueError(\"PDOStatement::setFetchMode(): Argument #1 (\\$mode) cannot use PDO::FETCH_CLASSTYPE, PDO::FETCH_PROPS_LATE, or PDO::FETCH_SERIALIZE fetch flags with a fetch mode other than PDO::FETCH_CLASS\");\n        }",
        )
        .replace(
            "if (($mode & 0x80) != 0 && $_base != 8) {\n            throw new ValueError('PDOStatement::fetch(): Argument #1 ($mode) must use PDO::FETCH_CLASSTYPE with PDO::FETCH_CLASS');",
            "if (($mode & (0x80 | 0x100 | 0x200)) != 0 && $_base != 8) {\n            throw new ValueError('PDOStatement::fetch(): Argument #1 ($mode) cannot use PDO::FETCH_CLASSTYPE, PDO::FETCH_PROPS_LATE, or PDO::FETCH_SERIALIZE fetch flags with a fetch mode other than PDO::FETCH_CLASS');",
        )
        .replace(
            "throw new ValueError(\"PDOStatement::fetchAll(): Argument #1 (\\$mode) cannot be PDO::FETCH_LAZY\");\n        }\n        if ($_base == 10) {",
            "throw new ValueError(\"PDOStatement::fetchAll(): Argument #1 (\\$mode) PDO::FETCH_LAZY cannot be used with PDOStatement::fetchAll()\");\n        }\n        if ($_base == 9) {\n            throw new ValueError(\"PDOStatement::fetchAll(): Argument #1 (\\$mode) PDO::FETCH_INTO cannot be used with PDOStatement::fetchAll()\");\n        }\n        if ($_base == 10) {",
        )
        .replace(
            "        // -- elephc PHP >= 8.5 SQLite setAuthorizer insertion --",
            "        public function setAuthorizer(?callable $callback): void {\n            // PHP 8.5+: null removes the native registration before its rooted\n            // descriptor is released. A callable reuses the scalar adapter because\n            // SQLite's authorizer ABI is five scalar arguments plus an integer result.\n            if ($callback === null) {\n                \\elephc_pdo_clear_authorizer($this->connectionId());\n                $this->authorizerCallback = function() { return 0; };\n                return;\n            }\n            if (!\\is_callable($callback)) {\n                throw new \\TypeError(\"Pdo\\\\Sqlite::setAuthorizer(): Argument #1 (\\$callback) must be a valid callback or null\");\n            }\n            $_normalized = \\__elephc_normalize_callable($callback);\n            $_descriptor = \\__elephc_callable_ptr($_normalized);\n            $_adapter = \\__elephc_pdo_adapter_addr(1);\n            if (\\elephc_pdo_set_authorizer($this->connectionId(), $_descriptor, $_adapter) !== 1) {\n                throw new \\PDOException(\"Failed to register SQLite authorizer\");\n            }\n            $this->authorizerCallback = $_normalized;\n        }",
        );
    if php_version >= PhpVersion::Php86 {
        source = source.replace(
            "elephc_pdo_release($this->conn, 0);",
            "elephc_pdo_release($this->conn, 1);",
        );
    }
    Cow::Owned(source)
}

/// Removes one inclusive source fragment delimited by stable version-gate comments.
/// Panics when either marker is missing because a renamed prelude marker must fail
/// compiler tests loudly instead of silently exposing a method in the wrong PHP version.
fn remove_version_block(source: &mut String, begin: &str, end: &str) {
    let start = source
        .find(begin)
        .unwrap_or_else(|| panic!("missing PDO prelude version-gate marker: {begin}"));
    let relative_end = source[start..]
        .find(end)
        .unwrap_or_else(|| panic!("missing PDO prelude version-gate marker: {end}"));
    let mut finish = start + relative_end + end.len();
    if source.as_bytes().get(finish) == Some(&b'\n') {
        finish += 1;
    }
    source.replace_range(start..finish, "");
}

#[cfg(test)]
mod version_tests {
    use super::*;

    /// Verifies the core ATTR_STATEMENT_CLASS contract remains present for every
    /// supported PHP compatibility target from 8.0 through 8.6.
    #[test]
    fn all_versions_keep_statement_class_support() {
        for version in PhpVersion::ALL {
            let source = prelude_source_for_version(version);
            assert!(source.contains("const ATTR_STATEMENT_CLASS = 13;"));
            assert!(source.contains("private array $statementClassConfig;"));
            assert!(source.contains("__elephc_pdo_statement_class_status"));
            assert!(source.contains("__elephc_invoke_pdo_statement_constructor"));
        }
    }

    /// Verifies PHP 8.4 keeps the historical high-bit fetch flag values and decoder mask.
    #[test]
    fn php84_source_keeps_high_fetch_flags() {
        let source = prelude_source_for_version(PhpVersion::Php84);
        assert!(source.contains("const FETCH_GROUP = 0x10000;"));
        assert!(source.contains("$_base = $mode & 0xFFFF;"));
    }

    /// Verifies PHP 8.0-8.3 retain legacy driver methods without exposing the
    /// namespaced PHP 8.4 classes or `PDO::connect()` factory.
    #[test]
    fn php83_source_uses_legacy_driver_surface() {
        let source = prelude_source_for_version(PhpVersion::Php83);
        assert!(source.contains("public function sqliteCreateFunction"));
        assert!(source.contains("public function pgsqlCopyFromArray"));
        assert!(!source.contains("public static function connect"));
        assert!(!source.contains("namespace Pdo {"));
        let tokens = crate::lexer::tokenize(source.as_ref()).expect("tokenize PHP 8.3 PDO prelude");
        crate::parser::parse(&tokens).expect("parse PHP 8.3 PDO prelude");
    }

    /// Verifies PHP 8.0 keeps query text as private implementation storage because
    /// PDOStatement/PDORow only gained public `queryString` properties in PHP 8.1.
    #[test]
    fn php80_source_hides_query_string_properties() {
        let source = prelude_source_for_version(PhpVersion::Php80);
        assert_eq!(source.matches("private string $queryString;").count(), 2);
        assert!(!source.contains("public readonly string $queryString;"));
        let tokens = crate::lexer::tokenize(source.as_ref()).expect("tokenize PHP 8.0 PDO prelude");
        crate::parser::parse(&tokens).expect("parse PHP 8.0 PDO prelude");
    }

    /// Verifies PHP 8.1 exposes both public query-string properties while retaining
    /// the legacy, non-namespaced driver surface used until PHP 8.3.
    #[test]
    fn php81_source_exposes_query_string_properties() {
        let source = prelude_source_for_version(PhpVersion::Php81);
        assert_eq!(source.matches("public readonly string $queryString;").count(), 2);
        assert!(!source.contains("namespace Pdo {"));
        assert!(!source.contains("#[\\SensitiveParameter]"));
        assert!(prelude_source_for_version(PhpVersion::Php82)
            .contains("#[\\SensitiveParameter] ?string $password"));
    }

    /// Verifies PHP 8.5 emits the compact flag values and updates every executable mask.
    #[test]
    fn php85_source_uses_compact_fetch_flags() {
        let source = prelude_source_for_version(PhpVersion::Php85);
        assert!(source.contains("const FETCH_GROUP = 0x20;"));
        assert!(source.contains("const FETCH_UNIQUE = 0x40;"));
        assert!(source.contains("const FETCH_PROPS_LATE = 0x100;"));
        assert!(source.contains("$_base = $mode & 0xF;"));
        assert!(!source.contains("$_base = $mode & 0xFFFF;"));
        assert!(source.contains("$_unique = ($mode & 0x40) != 0;"));
        assert!(source.contains("public function setAuthorizer(?callable $callback): void"));
        assert!(source.contains(
            "public function pgsqlCopyFromArray(string $tableName, array $rows,"
        ));
    }

    /// Verifies the generated PHP 8.5 prelude remains valid lexer and parser input.
    #[test]
    fn php85_source_tokenizes_and_parses() {
        let source = prelude_source_for_version(PhpVersion::Php85);
        assert!(source.contains(
            "#[\\Deprecated(\"as it has no effect\")]\n        const TRANSACTION_IDLE = 0;"
        ));
        assert!(!prelude_source_for_version(PhpVersion::Php84)
            .contains("#[\\Deprecated(\"as it has no effect\")]"));
        let tokens = crate::lexer::tokenize(source.as_ref()).expect("tokenize PHP 8.5 PDO prelude");
        crate::parser::parse(&tokens).expect("parse PHP 8.5 PDO prelude");
    }

    /// PHP 8.5+ alone enables lazy simple-query consumption on PostgreSQL statements.
    #[test]
    fn pgsql_simple_streaming_is_version_gated() {
        assert!(!prelude_source_for_version(PhpVersion::Php84)
            .contains("elephc_pdo_stmt_enable_simple_streaming($_handle)"));
        assert!(prelude_source_for_version(PhpVersion::Php85)
            .contains("elephc_pdo_stmt_enable_simple_streaming($_handle)"));
        assert!(prelude_source_for_version(PhpVersion::Php86)
            .contains("elephc_pdo_stmt_enable_simple_streaming($_handle)"));
    }

    /// Every supported PHP target generates syntactically valid PDO source, while
    /// PHP 8.6 alone enables PostgreSQL's new persistent-session reset behavior.
    #[test]
    fn every_version_source_tokenizes_and_php86_enables_session_reset() {
        for version in PhpVersion::ALL {
            let source = prelude_source_for_version(version);
            let tokens = crate::lexer::tokenize(source.as_ref())
                .unwrap_or_else(|error| panic!("tokenize PHP {version} PDO prelude: {error}"));
            crate::parser::parse(&tokens)
                .unwrap_or_else(|error| panic!("parse PHP {version} PDO prelude: {error}"));
        }
        assert!(prelude_source_for_version(PhpVersion::Php85)
            .contains("elephc_pdo_release($this->conn, 0);"));
        assert!(prelude_source_for_version(PhpVersion::Php86)
            .contains("elephc_pdo_release($this->conn, 1);"));
    }
}
