//! Purpose:
//! The shared `extern "elephc_pdo" { … }` declaration block, split out of
//! `PDO_PRELUDE_SRC` so that more than one PHP surface (PDO, mysqli) can
//! declare the same C-ABI bridge symbols without duplicating the block.
//!
//! Called from:
//! - `crate::pdo_prelude::inject_bridge_externs` (idempotent prepend used by
//!   both the PDO and mysqli prelude injection paths).
//! - `crate::pdo_prelude::bridge_externs_src` for source-level scans (parity
//!   gates over prelude sources).
//!
//! Key details:
//! - The block text is verbatim from the original `PDO_PRELUDE_SRC`, comments
//!   included; symbol names are ABI and must not be renamed here.
//! - Declaring these externs is what makes the type checker record the
//!   `elephc_pdo` staticlib as a required library, so any surface that injects
//!   this fragment automatically links the bridge.

/// The complete `extern "elephc_pdo"` declaration block as elephc-PHP source.
pub const SRC: &str = r#"<?php

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
    // v25 adds $my_found_rows and $persistent_key:
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
    // v41 adds $my_driver_config, the packed pdo_mysql connection controls for
    // LOCAL INFILE, compression, IGNORE_SPACE, multi-statements, buffering,
    // CAPATH, cipher suites, and a caller-supplied authentication public key.
    function elephc_pdo_open_persistent(string $dsn, int $persistent, int $sqlite_flags, string $my_init_command, string $my_ssl_config, int $my_found_rows, string $persistent_key, string $my_driver_config): int;
    function elephc_pdo_last_open_error(): string;
    // v54: driver-native constructor diagnostics, populated by CLI and PDO_OCI.
    function elephc_pdo_last_open_sqlstate(): string;
    function elephc_pdo_last_open_native_code(): int;
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
    // v53+: native OCI/CLI input/output binds and their length-counted result bytes.
    function elephc_pdo_bind_output(int $stmt, int $idx, int $type, int $maxLength): int;
    function elephc_pdo_output_data(int $stmt, int $idx): int;
    function elephc_pdo_output_is_lob(int $stmt, int $idx): int;
    function elephc_pdo_output_is_numeric(int $stmt, int $idx): int;
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
    // v49: writable/readable PDO_DBLIB-specific connection attributes.
    function elephc_pdo_dblib_set_attribute(int $conn, int $attribute, int $value): int;
    function elephc_pdo_dblib_attribute_bool(int $conn, int $attribute): int;
    function elephc_pdo_dblib_os_errcode(int $conn): int;
    function elephc_pdo_dblib_severity(int $conn): int;
    function elephc_pdo_dblib_os_errmsg(int $conn): string;
    function elephc_pdo_dblib_stmt_os_errcode(int $stmt): int;
    function elephc_pdo_dblib_stmt_severity(int $stmt): int;
    function elephc_pdo_dblib_stmt_os_errmsg(int $stmt): string;
    // v50: optional PDO_FIREBIRD connection attributes.
    function elephc_pdo_firebird_set_attribute_int(int $conn, int $attribute, int $value): int;
    function elephc_pdo_firebird_set_attribute_text(int $conn, int $attribute, string $value): int;
    function elephc_pdo_firebird_attribute_int(int $conn, int $attribute): int;
    function elephc_pdo_firebird_attribute_text(int $conn, int $attribute): string;
    function elephc_pdo_firebird_column_pdo_type(int $stmt, int $column): int;
    function elephc_pdo_firebird_stmt_set_cursor_name(int $stmt, string $name): int;
    function elephc_pdo_firebird_stmt_cursor_name(int $stmt): string;
    // v51: optional PDO_ODBC connection and statement attributes.
    function elephc_pdo_odbc_set_attribute(int $conn, int $attribute, int $value): int;
    function elephc_pdo_odbc_attribute(int $conn, int $attribute): int;
    function elephc_pdo_odbc_stmt_set_cursor_name(int $stmt, string $name): int;
    function elephc_pdo_odbc_stmt_cursor_name(int $stmt): string;
    function elephc_pdo_odbc_stmt_set_assume_utf8(int $stmt, int $enabled): int;
    function elephc_pdo_odbc_stmt_assume_utf8(int $stmt): int;
    // v52: optional PDO_OCI attributes and column metadata.
    function elephc_pdo_oci_set_attribute_int(int $conn, int $attribute, int $value): int;
    function elephc_pdo_oci_set_attribute_text(int $conn, int $attribute, string $value): int;
    function elephc_pdo_oci_attribute_int(int $conn, int $attribute): int;
    function elephc_pdo_oci_column_pdo_type(int $stmt, int $column): int;
    function elephc_pdo_oci_column_scale(int $stmt, int $column): int;
    function elephc_pdo_oci_column_flags(int $stmt, int $column): int;
    // v55: PECL PDO_INFORMIX's scale and UDT-only metadata parameter type.
    function elephc_pdo_informix_column_scale(int $stmt, int $column): int;
    function elephc_pdo_informix_column_pdo_type(int $stmt, int $column): int;
    // v56: PECL PDO_IBM 1.7.0 connection attributes and CLI column metadata.
    function elephc_pdo_ibm_set_attribute_text(int $conn, int $attribute, string $value): int;
    function elephc_pdo_ibm_attribute_text(int $conn, int $attribute): string;
    function elephc_pdo_ibm_attribute_int(int $conn, int $attribute): int;
    function elephc_pdo_ibm_column_scale(int $stmt, int $column): int;
    function elephc_pdo_ibm_column_pdo_type(int $stmt, int $column): int;
    // v57: Microsoft PDO_SQLSRV 5.13.1 attributes and information arrays.
    function elephc_pdo_sqlsrv_stmt_set_attribute(int $stmt, int $attribute, int $value): int;
    function elephc_pdo_sqlsrv_stmt_configure(int $stmt, int $attribute, int $value): int;
    function elephc_pdo_sqlsrv_stmt_attribute(int $stmt, int $attribute): int;
    function elephc_pdo_sqlsrv_column_is_datetime(int $stmt, int $column): int;
    function elephc_pdo_sqlsrv_info(int $conn, int $field): string;
    function elephc_pdo_sqlsrv_classification_pair_count(int $stmt, int $column): int;
    function elephc_pdo_sqlsrv_classification_text(int $stmt, int $column, int $pair, int $field): string;
    function elephc_pdo_sqlsrv_classification_pair_rank(int $stmt, int $column, int $pair): int;
    function elephc_pdo_sqlsrv_classification_query_rank(int $stmt): int;
    // v58: official PDO_CUBRID connection attributes, schema API, metadata, and quoter.
    function elephc_pdo_cubrid_set_attribute(int $conn, int $attribute, int $value): int;
    function elephc_pdo_cubrid_attribute(int $conn, int $attribute): int;
    function elephc_pdo_cubrid_quote(int $conn, string $data, int $length): int;
    function elephc_pdo_cubrid_bind_typed(int $stmt, int $index, string $data, int $length, string $typeName, int $isSet, int $pdoType): int;
    function elephc_pdo_cubrid_schema(int $conn, int $schemaType, string $className, string $attributeName): int;
    function elephc_pdo_cubrid_column_scale(int $stmt, int $column): int;
    function elephc_pdo_cubrid_column_default(int $stmt, int $column): string;
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
    // v59: mysqli-surface helpers over the same MySQL connection. thread_id and
    // param_count come straight from the handshake / prepared statement (no
    // round-trip); real_escape_string is charset-aware (closes the GBK/Big5
    // trailing-byte breakout that pure byte substitution leaves open) and writes
    // the escaped bytes to the shared blob cell (read via blob_data_ptr);
    // sql_has_multiple_statements is the one authoritative multi-statement
    // scanner (with `/*! … */` executable comments treated as live SQL).
    function elephc_pdo_mysql_thread_id(int $conn): int;
    function elephc_pdo_mysql_param_count(int $stmt): int;
    function elephc_pdo_mysql_charset(int $conn): string;
    function elephc_pdo_sql_has_multiple_statements(int $conn, string $sql, int $len): int;
    function elephc_pdo_real_escape_string(int $conn, string $data, int $len): int;
    // v60: the rest of the mysqli connection surface over the same client: the
    // last OK packet's info string ($info / mysqli_info()), a real COM_PING, a
    // real COM_CHANGE_USER ($has_db = 0 keeps no database selected), and the
    // server-side id of a native prepare (mysqli_stmt::$id), plus the linked client
    // library's version string with no connection (mysqli_get_client_info()).
    function elephc_pdo_mysql_info(int $conn): string;
    function elephc_pdo_mysql_client_info(): string;
    function elephc_pdo_mysql_ping(int $conn): int;
    function elephc_pdo_mysql_change_user(int $conn, string $user, string $pass, string $db, int $has_db): int;
    function elephc_pdo_mysql_stmt_id(int $stmt): int;
    // v61: MYSQLI_ASYNC on a worker thread — send returns at once, wait blocks up to
    // $timeout_us microseconds (1 = ready, 0 = timeout, -1 = none in flight), and reap
    // hands back an executed statement holding the buffered result (-1 = query failed).
    function elephc_pdo_mysql_async_send(int $conn, string $sql): int;
    function elephc_pdo_mysql_async_wait(int $conn, int $timeout_us): int;
    function elephc_pdo_mysql_async_reap(int $conn): int;
    // v62: raw protocol commands and the OK-packet server status flags (patched mysql
    // crate, see third_party/mysql-28.0.4/ELEPHC_PATCH.md).
    function elephc_pdo_mysql_server_status(int $conn): int;
    function elephc_pdo_mysql_refresh(int $conn, int $options): int;
    function elephc_pdo_mysql_dump_debug_info(int $conn): int;
    function elephc_pdo_mysql_io_stat(int $conn, int $which): int;
    function elephc_pdo_mysql_set_server_option(int $conn, int $option): int;
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
    // v49: the remaining PDO_DBLIB `getColumnMeta()` descriptor fields.
    function elephc_pdo_dblib_column_native_type_id(int $stmt, int $i): int;
    function elephc_pdo_dblib_column_user_type_id(int $stmt, int $i): int;
    function elephc_pdo_dblib_column_scale(int $stmt, int $i): int;
    function elephc_pdo_dblib_column_source(int $stmt, int $i): string;
}
"#;
