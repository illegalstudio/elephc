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
    function elephc_pdo_open_persistent(string $dsn, int $persistent, int $sqlite_flags, string $my_init_command, string $my_ssl_config): int;
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
    function elephc_pdo_column_text(int $stmt, int $i): string;
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
    // return its byte length (-1 on error); blob_byte drains that buffer one byte at a
    // time so embedded NUL bytes survive into the PHP string.
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

    // Divergence from PHP's (message, code, previous) signature: the native-code
    // slot is dropped, because the base Exception $code is int-typed and cannot
    // hold a 5-character SQLSTATE string, so the SQLSTATE travels in errorInfo[0]
    // instead. getCode() therefore reports the base default rather than the
    // SQLSTATE string; read $e->errorInfo[0] for the SQLSTATE.
    public function __construct(string $message = "", ?array $errorInfo = null) {
        // The built-in Exception constructor is a checker-synthesized method with
        // no linkable symbol, so `parent::__construct()` cannot be called; the
        // public `$message` property (see getMessage()) is assigned directly.
        $this->message = $message;
        $this->errorInfo = $errorInfo;
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

    public function __construct(string $dsn, ?string $username = null, ?string $password = null, ?array $options = null) {
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
        // Constructor options affect the connection that is opened below, so
        // apply them before the bridge sees the DSN. In particular,
        // ATTR_PERSISTENT selects the bridge's process-local DSN pool.
        if ($options !== null) {
            foreach ($options as $_attr => $_val) {
                $_iattr = (int) $_attr;
                if ($_iattr == 3) {
                    // P1-h: same ATTR_ERRMODE value validation as setAttribute() below —
                    // a bad mode must not silently take effect via the constructor either.
                    $this->checkErrMode((int) $_val);
                    $this->errMode = (int) $_val;
                } elseif ($_iattr == 12) {
                    $this->persistent = (bool) $_val;
                } elseif ($_iattr == 19) {
                    // P1-h: same ATTR_DEFAULT_FETCH_MODE validation as setAttribute() below.
                    $this->checkDefaultFetchMode((int) $_val);
                    $this->defaultFetchMode = (int) $_val;
                } elseif ($_iattr == 17) {
                    $this->stringifyFetches = (bool) $_val;
                } elseif ($_iattr == 8) {
                    // P2-e: same ATTR_CASE value validation as setAttribute() below.
                    $this->checkAttrCase((int) $_val);
                    $this->attrCase = (int) $_val;
                } elseif ($_iattr == 11) {
                    $this->oracleNulls = (int) $_val;
                } elseif ($_iattr == 1000) {
                    $_openFlags = (int) $_val;
                } elseif ($_iattr == 1002) {
                    $_myInitCommand = (string) $_val;
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
        // SQLite ignores credentials. For PostgreSQL and MySQL, the user/password
        // may be passed as the PDO constructor arguments (PHP-style); fold them
        // into the DSN's `key=value` list, where the bridge parses them. P2-6:
        // DSN-embedded credentials win over the constructor arguments (PHP
        // parity) — only append a key the DSN does not already carry, instead of
        // unconditionally duplicating it.
        $_dsn = $dsn;
        if (str_starts_with($dsn, "pgsql:") || str_starts_with($dsn, "mysql:")) {
            if ($username !== null && !str_contains($dsn, "user=")) {
                $_dsn = $_dsn . ";user=" . $username;
            }
            if ($password !== null && !str_contains($dsn, "password=")) {
                $_dsn = $_dsn . ";password=" . $password;
            }
            // P2-1: ATTR_TIMEOUT maps to the driver's connect-time socket
            // timeout. libpq's `connect_timeout` conninfo key and the mysql
            // client's `connect_timeout` DSN key (mapped to
            // OptsBuilder::tcp_connect_timeout in my.rs) are both plain
            // `key=value` pairs their respective parsers already understand, so
            // folding this into the DSN needs no further bridge change — only
            // applied when the DSN does not already specify it.
            if (isset($this->attributes[2]) && !str_contains($dsn, "connect_timeout=")) {
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
        $this->conn = elephc_pdo_open_persistent($_dsn, $this->persistent ? 1 : 0, $_openFlags, $_myInitCommand, $_mySslConfig);
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
            // default) otherwise; native code is unknown here (null). An
            // unrecognized DSN prefix is a different failure (no driver even
            // attempted the connection) and keeps PHP's bare "could not find
            // driver" shape — no SQLSTATE prefix, errorInfo stays null
            // (verified against a real PHP 8.5 CLI). The `null` is passed
            // EXPLICITLY (not left to the constructor's default): a bare
            // `new PDOException($msg)` omitting the second argument does not
            // actually read back as `null` (a pre-existing, general
            // default-argument-materialization bug, reproducible with plain
            // `throw new PDOException("x")` — unrelated to this fix), so an
            // explicit `null` here is required for the omitted-driver branch to
            // truly leave `errorInfo` null.
            if (str_starts_with($_dsn, "sqlite:") || str_starts_with($_dsn, "pgsql:") || str_starts_with($_dsn, "mysql:")) {
                $_sqlstate = str_starts_with($_dsn, "sqlite:") ? "HY000" : "08006";
                throw new PDOException("SQLSTATE[" . $_sqlstate . "]: " . $_openMsg, [$_sqlstate, null, $_openMsg]);
            }
            throw new PDOException($_openMsg, null);
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

    public function setAttribute(int $attribute, $value): bool {
        if ($attribute == 3) {
            $this->checkErrMode((int) $value);
            $this->errMode = (int) $value;
        } elseif ($attribute == 12) {
            $this->persistent = (bool) $value;
        } elseif ($attribute == 2) {
            // ATTR_TIMEOUT: SQLite maps it to a busy-timeout; PHP's unit is
            // seconds, SQLite's is milliseconds. Other drivers accept it as a
            // no-op (see the bridge).
            elephc_pdo_set_busy_timeout($this->conn, ((int) $value) * 1000);
        } elseif ($attribute == 19) {
            $this->checkDefaultFetchMode((int) $value);
            $this->defaultFetchMode = (int) $value;
        } elseif ($attribute == 17) {
            $this->stringifyFetches = (bool) $value;
        } elseif ($attribute == 8) {
            $this->checkAttrCase((int) $value);
            $this->attrCase = (int) $value;
        } elseif ($attribute == 11) {
            $this->oracleNulls = (int) $value;
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
        if (isset($this->attributes[$attribute])) {
            return $this->attributes[$attribute];
        }
        return null;
    }

    public function exec(string $statement): int|bool {
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

    public function lastInsertId(?string $name = null): string {
        // The name is a sequence for PostgreSQL (`currval($name)`); SQLite and
        // MySQL ignore it and return the last rowid / auto-increment id. The text
        // bridge is used so oversized PostgreSQL sequence values (which need not
        // fit in an i64) round-trip without truncation.
        return elephc_pdo_last_insert_id_text($this->conn, $name ?? "");
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
        // Divergence: the DSN prefix, not the receiver class, selects the driver, so
        // a subclass-qualified mismatched call (`Pdo\Sqlite::connect("mysql:...")`)
        // is not rejected here as PHP would.
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
        // signals a bridge error and yields false. The buffer is drained one byte at
        // a time (via chr()) so embedded NUL bytes survive into the PHP string, which
        // is then wrapped in a rewound in-memory read/write stream. This is a
        // read-whole snapshot: writing back to the stream does not update the stored
        // BLOB / large object.
        if ($length < 0) {
            return false;
        }
        $_data = "";
        for ($_j = 0; $_j < $length; $_j++) {
            $_data = $_data . \chr(\elephc_pdo_blob_byte($_j));
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
}

class PDOStatement implements Iterator {
    private int $stmt;
    private int $conn;
    private int $errMode;
    private int $fetchMode;
    private $fetchTarget;
    private array $boundParams;
    private array $boundValues;
    private array $boundTypes;
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
    public string $queryString;
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
        $this->boundValues = [];
        $this->boundTypes = [];
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
        if ($mode == 7 && $classOrColumn !== null && ((int) $classOrColumn) < 0) {
            throw new ValueError("PDOStatement::setFetchMode(): Argument #2 (\$args) must be greater than or equal to 0");
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
        if ($mode == 7 && $classOrColumn === null) {
            throw new ValueError("PDOStatement::setFetchMode() expects exactly 2 arguments for the fetch mode provided, 1 given");
        }
        if ($mode == 8 && $classOrColumn === null) {
            throw new ValueError("PDOStatement::setFetchMode() expects at least 2 arguments for the fetch mode provided, 1 given");
        }
        if ($mode == 9 && $classOrColumn === null) {
            throw new ValueError("PDOStatement::setFetchMode() expects exactly 2 arguments for the fetch mode provided, 1 given");
        }
        $this->fetchMode = $mode;
        if ($mode == 7 && $classOrColumn !== null) {
            $this->fetchColumn = (int) $classOrColumn;
        } elseif (($mode == 8 || $mode == 9) && $classOrColumn !== null) {
            $this->fetchTarget = $classOrColumn;
        }
        return true;
    }

    public function bindValue($parameter, $value, int $type = 2): bool {
        // Resolve the 1-based slot index now and record it. The named-placeholder
        // lookup must not be interleaved with value binds in execute()'s loop: a
        // loop body that branches between "lookup index" and "no lookup" corrupts
        // a sibling bind in generated code. Recording resolved int slots keeps
        // execute()'s bind loop uniform.
        if (is_int($parameter)) {
            $_slot = (int) $parameter;
        } else {
            $_slot = (int) elephc_pdo_bind_parameter_index($this->stmt, (string) $parameter);
        }
        $this->boundParams[] = $_slot;
        $this->boundValues[] = $value;
        $this->boundTypes[] = $type;
        return true;
    }

    public function bindParam($parameter, $variable, int $type = 2, int $maxLength = 0, mixed $driverOptions = null): bool {
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
                $_btype = $this->boundTypes[$_i];
                if ($_btype == 0 || is_null($_value)) {
                    elephc_pdo_bind_null($this->stmt, $_slot);
                } elseif ($_btype == 1 || $_btype == 5) {
                    elephc_pdo_bind_int($this->stmt, $_slot, (int) $_value);
                } elseif ($_btype == 3) {
                    // PDO::PARAM_LOB: route through bind_blob (raw bytes, embedded
                    // NUL preserved) rather than bind_text.
                    $_s = (string) $_value;
                    elephc_pdo_bind_blob($this->stmt, $_slot, $_s, strlen($_s));
                } elseif ($_btype == 100) {
                    // P2 (not a real PDO::PARAM_* value): an internal marker
                    // recorded only by execute($params)'s array-bind rebuild
                    // below, for a PHP float element, so a later no-arg
                    // execute() replay re-binds it as a double instead of
                    // falling into the text branch and stringifying it.
                    elephc_pdo_bind_double($this->stmt, $_slot, (float) $_value);
                } else {
                    // PDO::PARAM_STR (and anything else): bind_text with the
                    // measured byte length so an embedded NUL byte survives.
                    $_s = (string) $_value;
                    elephc_pdo_bind_text($this->stmt, $_slot, $_s, strlen($_s));
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
            $this->boundValues = [];
            $this->boundTypes = [];
            // Apply this call's parameter array (positional ? and named :name).
            foreach ($params as $_key => $_pv) {
                if (is_int($_key)) {
                    $_idx = $_key + 1;
                } else {
                    $_idx = elephc_pdo_bind_parameter_index($this->stmt, (string) $_key);
                }
                $_pslot = (int) $_idx;
                if (is_int($_pv)) {
                    elephc_pdo_bind_int($this->stmt, $_pslot, (int) $_pv);
                    $this->boundTypes[] = 1;
                } elseif (is_bool($_pv)) {
                    elephc_pdo_bind_int($this->stmt, $_pslot, (int) $_pv);
                    $this->boundTypes[] = 1;
                } elseif (is_float($_pv)) {
                    elephc_pdo_bind_double($this->stmt, $_pslot, (float) $_pv);
                    // 100: see the replay loop's matching comment above.
                    $this->boundTypes[] = 100;
                } elseif (is_null($_pv)) {
                    elephc_pdo_bind_null($this->stmt, $_pslot);
                    $this->boundTypes[] = 0;
                } else {
                    // The array-bind path carries no PDO type, so PARAM_STR /
                    // length-safe TEXT (embedded NUL preserved) is correct here.
                    $_ps = (string) $_pv;
                    elephc_pdo_bind_text($this->stmt, $_pslot, $_ps, strlen($_ps));
                    $this->boundTypes[] = 2;
                }
                $this->boundParams[] = $_pslot;
                $this->boundValues[] = $_pv;
            }
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
        $_len = elephc_pdo_column_data_len($this->stmt, $index);
        $_out = "";
        for ($_j = 0; $_j < $_len; $_j++) {
            $_out = $_out . chr(elephc_pdo_column_data_byte($this->stmt, $index, $_j));
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
        for ($_i = 0; $_i < $count; $_i++) {
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

    public function fetch(int $mode = 0, mixed $classOrObject = null, int $cursorOffset = 0): mixed {
        // $cursorOffset (PHP's $cursorOrientation-paired scroll offset) is accepted
        // for signature compatibility with PHP's 3-arg `fetch()` but ignored — the
        // bridge's cursor is forward-only (PDO::CURSOR_FWDONLY), matching every
        // driver here, so there is nothing to seek to.
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
        // P2: real php-src's `pdo_stmt_verify_mode` raises a ValueError (not a
        // PDOException) for every one of these mode-validation failures; a
        // PDOException here would be uncatchable by code written against real
        // PHP's `catch (\ValueError $e)`.
        if ($_base == 1) {
            throw new ValueError("PDO::FETCH_LAZY is not supported");
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
            if ($classOrObject !== null) {
                return $this->assignColumns(new $classOrObject(), $_count);
            }
            if ($this->fetchTarget !== null) {
                $_classTarget = $this->fetchTarget;
                return $this->assignColumns(new $_classTarget(), $_count);
            }
            return $this->assignColumns(new stdClass(), $_count);
        }
        if ($_base == 9) {
            if ($classOrObject !== null) {
                return $this->assignColumns($classOrObject, $_count);
            }
            if ($this->fetchTarget !== null) {
                return $this->assignColumns($this->fetchTarget, $_count);
            }
            return $this->assignColumns(new stdClass(), $_count);
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
        // signature compatibility but not forwarded — new $classOrObject() below
        // (via fetch()) is always built with no arguments, the same documented
        // divergence as fetchObject()'s $constructorArgs.
        $_unusedCtorArgs = $ctorArgs;
        if ($mode == 0) {
            $mode = $this->fetchMode;
        }
        $_base = $mode & 0xFFFF;
        // FETCH_GROUP (0x10000) / FETCH_UNIQUE (0x30000) reshape the whole result
        // set; returning a flat list would be silently wrong, so fail loudly until
        // they are implemented.
        if (($mode & 0x10000) != 0) {
            throw new PDOException("PDO::FETCH_GROUP and PDO::FETCH_UNIQUE are not yet supported");
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
        if ($_base == 7 && $classOrObject !== null) {
            // FETCH_COLUMN: PHP applies the 2nd argument as the column index for this
            // call (`stmt->fetch.column = Z_LVAL(arg2)`). fetch()'s FETCH_COLUMN branch
            // reads $this->fetchColumn, so set it here (mirroring setFetchMode) before
            // the row loop — otherwise fetchAll(PDO::FETCH_COLUMN, $n) silently returns
            // column 0's data regardless of $n.
            $this->fetchColumn = (int) $classOrObject;
        }
        $_rows = [];
        while (true) {
            $_row = $this->fetch($mode, $classOrObject);
            if ($_row === false) {
                break;
            }
            $_rows[] = $_row;
        }
        return $_rows;
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
        // native_type/pdo_type. PHP's remaining metadata (len, precision, table) is
        // not surfaced by the bridge, so those keys are present with neutral values
        // rather than omitted, so callers that read them do not error. Returns
        // false for an out-of-range column index.
        //
        // P2-h: also false when the statement hasn't been executed yet — there is
        // no result set (or, for a non-SELECT statement, no columns) to describe.
        //
        // P2-k (KNOWN LIMITATION, documented rather than fixed in this slice): for a
        // `pgsql:` statement this still reports the generic SQLite-storage-class
        // metadata above (native_type "integer"/"double"/"string", no "blob" flag for
        // a real `bytea` column) instead of PostgreSQL's actual native_type
        // (`int4`/`bool`/`bytea`/…) or the `pgsql:oid`/`pgsql:table` keys php-src's
        // pdo_pgsql reports (ext/pdo_pgsql/pgsql_statement.c, via `PQftype`/
        // `PQftable`). A real fix means threading the bridge's
        // `postgres::types::Type` (available per-column off the prepared
        // `Statement`'s `columns()`, already retained on `PgStmt`) through a new
        // driver-aware accessor and an OID→name/pdo_type table here — involved
        // enough that it is out of scope for this slice; `pdo_type`/"flags" for a
        // pg BOOL or bytea column are therefore currently wrong (never
        // PARAM_BOOL/PARAM_LOB).
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
        // Reduced PDOStatement::debugDumpParams: writes the SQL and the number of
        // bound parameters to stdout. PHP additionally prints per-parameter detail;
        // that format is a debugging aid and is not contractual, so elephc emits the
        // load-bearing SQL + parameter count. Always returns null (never false here,
        // as elephc keeps no unparsed-query state PHP would report failure for).
        echo "SQL: [" . strlen($this->queryString) . "] " . $this->queryString . "\n";
        echo "Params:  " . count($this->boundValues) . "\n";
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
        // prelude implements `Iterator` directly instead (see the comment above
        // rewind()), so the statement itself already satisfies that contract.
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
            // Forward to \PDO to open the connection, then initialise the callback
            // root (an uninitialised typed array property is not implicitly []).
            parent::__construct($dsn, $username, $password, $options);
            $this->udfCallbacks = [];
        }

        public function loadExtension(string $name): void {
            // Loads a SQLite extension library by path (its entry point is
            // auto-derived, as PHP's loadExtension does), throwing on failure.
            // Extension loading runs native code from the named library, so it
            // weakens the standalone-binary guarantee — use only trusted extensions.
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
            // Forward to \PDO to open the connection, then seed a no-op callback so
            // drainNotices() always has a callable to hand each notice to (a notice
            // arriving before setNoticeCallback() is drained and discarded).
            parent::__construct($dsn, $username, $password, $options);
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
            if ($separator === "\t" && $nullAs === "\\N") {
                return "";
            }
            $_delim = $separator === "\t" ? "E'\\t'" : "'" . $separator . "'";
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
