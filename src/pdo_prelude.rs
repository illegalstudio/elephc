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
    function elephc_pdo_open_persistent(string $dsn, int $persistent): int;
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
    function elephc_pdo_bind_text(int $stmt, int $idx, string $val): int;
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
}

class PDOException extends RuntimeException {
    // PHP surfaces the [SQLSTATE, driver-specific code, message] triple here;
    // frameworks (Doctrine, Laravel) read $e->errorInfo[0] for the SQLSTATE. It
    // is untyped to match the proven exception-subclass extra-property shape and
    // to allow the null "no structured info" case (e.g. connection-open failures).
    public $errorInfo;

    // Divergence from PHP's (message, code, previous) signature: the native-code
    // slot is dropped, because the base Exception $code is int-typed and cannot
    // hold a 5-character SQLSTATE string, so the SQLSTATE travels in errorInfo[0]
    // instead. getCode() therefore reports the base default rather than the
    // SQLSTATE string; read $e->errorInfo[0] for the SQLSTATE.
    public function __construct(string $message = "", $errorInfo = null) {
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

    public function __construct(string $dsn, ?string $username = null, ?string $password = null, ?array $options = null) {
        $this->errMode = 2;
        $this->persistent = false;
        $this->attributes = [];
        $this->inTxn = false;
        $this->defaultFetchMode = 4;
        // Constructor options affect the connection that is opened below, so
        // apply them before the bridge sees the DSN. In particular,
        // ATTR_PERSISTENT selects the bridge's process-local DSN pool.
        if ($options !== null) {
            foreach ($options as $_attr => $_val) {
                $_iattr = (int) $_attr;
                if ($_iattr == 3) {
                    $this->errMode = (int) $_val;
                } elseif ($_iattr == 12) {
                    $this->persistent = (bool) $_val;
                } elseif ($_iattr == 19) {
                    $this->defaultFetchMode = (int) $_val;
                }
                $this->attributes[$_iattr] = $_val;
            }
        }
        // SQLite ignores credentials. For PostgreSQL and MySQL, the user/password
        // may be passed as the PDO constructor arguments (PHP-style); fold them
        // into the DSN's `key=value` list, where the bridge parses them (a `user=`
        // / `password=` already in the DSN is overridden by the explicit argument).
        $_dsn = $dsn;
        if (str_starts_with($dsn, "pgsql:") || str_starts_with($dsn, "mysql:")) {
            if ($username !== null) {
                $_dsn = $_dsn . ";user=" . $username;
            }
            if ($password !== null) {
                $_dsn = $_dsn . ";password=" . $password;
            }
        }
        $this->conn = elephc_pdo_open_persistent($_dsn, $this->persistent ? 1 : 0);
        if ($this->conn < 0) {
            throw new PDOException(elephc_pdo_last_open_error());
        }
        // ATTR_TIMEOUT needs a live connection, so apply it after the open (the
        // pre-open loop only records it). PHP's value is in seconds; SQLite's
        // busy-timeout is milliseconds.
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

    public function setAttribute(int $attribute, $value): bool {
        if ($attribute == 3) {
            $this->errMode = (int) $value;
        } elseif ($attribute == 12) {
            $this->persistent = (bool) $value;
        } elseif ($attribute == 2) {
            // ATTR_TIMEOUT: SQLite maps it to a busy-timeout; PHP's unit is
            // seconds, SQLite's is milliseconds. Other drivers accept it as a
            // no-op (see the bridge).
            elephc_pdo_set_busy_timeout($this->conn, ((int) $value) * 1000);
        } elseif ($attribute == 19) {
            $this->defaultFetchMode = (int) $value;
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
        if ($attribute == 4) {
            return elephc_pdo_server_version($this->conn);
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

    public function prepare(string $query): PDOStatement|bool {
        $_handle = elephc_pdo_prepare($this->conn, $query);
        if ($_handle < 0) {
            $this->fail(elephc_pdo_errmsg($this->conn));
            return false;
        }
        // Inherit the connection's default fetch mode (ATTR_DEFAULT_FETCH_MODE) so
        // a statement fetched with no explicit mode uses the dbh default.
        $_stmt = new PDOStatement($_handle, $this->conn, $this->errMode, $query);
        $_stmt->setFetchMode($this->defaultFetchMode);
        return $_stmt;
    }

    public function query(string $query): PDOStatement|bool {
        $_statement = $this->prepare($query);
        if ($_statement === false) {
            return false;
        }
        if ($_statement->execute() === false) {
            return false;
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
        // logic error and throws regardless of the error mode.
        if ($this->inTxn) {
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
        // Driver-aware string-literal quoting. The $type argument is accepted for
        // PHP signature compatibility but ignored. Prepared statements remain the
        // recommended path; quote() is only safe when it matches the target
        // driver's literal syntax, so it branches on the driver name.
        $_unused = $type;
        $_driver = elephc_pdo_driver_name($this->conn);
        if ($_driver === "mysql") {
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
            return "'" . $_s . "'";
        }
        if ($_driver === "pgsql") {
            // PostgreSQL: double single quotes; if a backslash is present, use the
            // E'...' escape-string form so backslashes are taken literally
            // regardless of standard_conforming_strings.
            $_doubled = str_replace("'", "''", $string);
            if (strpos($string, "\\") !== false) {
                return "E'" . str_replace("\\", "\\\\", $_doubled) . "'";
            }
            return "'" . $_doubled . "'";
        }
        // SQLite (and the default): standard SQL ''-doubling is correct.
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
    private array $attributes;
    public string $queryString;

    public function __construct(int $handle, int $connection, int $errMode = 2, string $query = "") {
        $this->stmt = $handle;
        $this->conn = $connection;
        $this->errMode = $errMode;
        // PHP exposes the prepared SQL as the public PDOStatement::$queryString
        // property; thread it through from prepare() so debugDumpParams and callers
        // can read it.
        $this->queryString = $query;
        // Statement-level attribute store for get/setAttribute (a small per-object
        // map; PHP surfaces a handful of driver attributes here).
        $this->attributes = [];
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

    public function setFetchMode(int $mode, mixed $classOrColumn = null): bool {
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

    public function bindParam($parameter, $variable, int $type = 2): bool {
        // Unlike PHP, the value is recorded now (not read by reference at execute
        // time): bind right before execute(), or use bindValue().
        return $this->bindValue($parameter, $variable, $type);
    }

    public function execute(?array $params = null): bool {
        $this->executed = true;
        elephc_pdo_reset($this->stmt);
        elephc_pdo_clear_bindings($this->stmt);
        // Apply bindValue()/bindParam() bindings recorded since construction.
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
            } else {
                elephc_pdo_bind_text($this->stmt, $_slot, (string) $_value);
            }
        }
        // Apply this call's parameter array (positional ? and named :name).
        if ($params !== null) {
            foreach ($params as $_key => $_pv) {
                if (is_int($_key)) {
                    $_idx = $_key + 1;
                } else {
                    $_idx = elephc_pdo_bind_parameter_index($this->stmt, (string) $_key);
                }
                $_pslot = (int) $_idx;
                if (is_int($_pv)) {
                    elephc_pdo_bind_int($this->stmt, $_pslot, (int) $_pv);
                } elseif (is_bool($_pv)) {
                    elephc_pdo_bind_int($this->stmt, $_pslot, (int) $_pv);
                } elseif (is_float($_pv)) {
                    elephc_pdo_bind_double($this->stmt, $_pslot, (float) $_pv);
                } elseif (is_null($_pv)) {
                    elephc_pdo_bind_null($this->stmt, $_pslot);
                } else {
                    elephc_pdo_bind_text($this->stmt, $_pslot, (string) $_pv);
                }
            }
        }
        // A statement with no result columns (INSERT/UPDATE/DELETE/DDL) is run
        // now; SELECT-style statements (column_count > 0) are stepped lazily by
        // fetch() so the first row is not consumed here.
        if (elephc_pdo_column_count($this->stmt) == 0) {
            $_step = elephc_pdo_step($this->stmt);
            if ($_step < 0) {
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
        return true;
    }

    private function columnValue(int $index): mixed {
        $_type = elephc_pdo_column_type($this->stmt, $index);
        if ($_type == 1) {
            return elephc_pdo_column_int($this->stmt, $index);
        } elseif ($_type == 2) {
            return elephc_pdo_column_double($this->stmt, $index);
        } elseif ($_type == 5) {
            return null;
        }
        $_len = elephc_pdo_column_data_len($this->stmt, $index);
        $_out = "";
        for ($_j = 0; $_j < $_len; $_j++) {
            $_out = $_out . chr(elephc_pdo_column_data_byte($this->stmt, $index, $_j));
        }
        return $_out;
    }

    private function assignColumns(mixed $object, int $count): mixed {
        for ($_i = 0; $_i < $count; $_i++) {
            $_value = $this->columnValue($_i);
            $_name = elephc_pdo_column_name($this->stmt, $_i);
            $object->{$_name} = $_value;
        }
        return $object;
    }

    public function fetch(int $mode = 0, mixed $classOrObject = null): mixed {
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
        if ($_base == 1) {
            throw new PDOException("PDO::FETCH_LAZY is not supported");
        }
        $_rc = elephc_pdo_step($this->stmt);
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
            // FETCH_KEY_PAIR: exactly two columns map to [col0 => col1].
            if ($_count != 2) {
                throw new PDOException("SQLSTATE[HY000]: General error: PDO::FETCH_KEY_PAIR fetch mode requires 2 columns");
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
                $_name = elephc_pdo_column_name($this->stmt, $_i);
                $_assocRow[$_name] = $this->columnValue($_i);
            }
            return $_assocRow;
        }
        $_bothRow = [];
        for ($_i = 0; $_i < $_count; $_i++) {
            $_name = elephc_pdo_column_name($this->stmt, $_i);
            $_value = $this->columnValue($_i);
            $_bothRow[$_name] = $_value;
            $_bothRow[$_i] = $_value;
        }
        return $_bothRow;
    }

    public function fetchAll(int $mode = 0, mixed $classOrObject = null): array {
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
        if ($_base == 12) {
            // FETCH_KEY_PAIR: aggregate the two-column result into [col0 => col1].
            // Stepped directly (not via fetch()) so the map is built exactly like
            // FETCH_ASSOC, avoiding an intermediate single-entry return array.
            if (!$this->executed) {
                return [];
            }
            $_pairs = [];
            while (true) {
                $_krc = elephc_pdo_step($this->stmt);
                if ($_krc < 0) {
                    $this->fail(elephc_pdo_errmsg($this->conn));
                    break;
                }
                if ($_krc == 0) {
                    break;
                }
                if (elephc_pdo_column_count($this->stmt) != 2) {
                    throw new PDOException("SQLSTATE[HY000]: General error: PDO::FETCH_KEY_PAIR fetch mode requires 2 columns");
                }
                $_kk = $this->columnValue(0);
                $_vv = $this->columnValue(1);
                $_pairs[$_kk] = $_vv;
            }
            return $_pairs;
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
        $_rc = elephc_pdo_step($this->stmt);
        if ($_rc < 0) {
            $this->fail(elephc_pdo_errmsg($this->conn));
            return false;
        }
        if ($_rc == 0) {
            return false;
        }
        return $this->columnValue($column);
    }

    public function closeCursor(): bool {
        // Free the result set and require a re-execute before the next fetch,
        // matching PHP: after closeCursor() a fetch on the forward-only cursor
        // returns false until execute() runs again.
        elephc_pdo_reset($this->stmt);
        $this->executed = false;
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
        $_rc = elephc_pdo_step($this->stmt);
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
        // Statement-level attributes are a simple per-statement store. Returns null
        // for an attribute never set, matching PHP for an unknown statement attribute.
        if (isset($this->attributes[$name])) {
            return $this->attributes[$name];
        }
        return null;
    }

    public function setAttribute(int $attribute, mixed $value): bool {
        $this->attributes[$attribute] = $value;
        return true;
    }

    public function nextRowset(): bool {
        // elephc's drivers expose a single result set per prepared statement (SQLite
        // has no multiple rowsets; the pg/mysql bridges run one statement per
        // prepare), so there is never a further rowset. PHP returns false when no
        // more rowsets exist.
        return false;
    }

    public function getColumnMeta(int $column): array|bool {
        // Reduced PDOStatement::getColumnMeta: the column name plus the PDO and
        // native type derived from the bridge's per-column type code (1=INTEGER,
        // 2=FLOAT, 3=TEXT, 4=BLOB, 5=NULL). PHP's full metadata (len, precision,
        // driver flags, table) is not surfaced by the bridge, so those keys are
        // present with neutral values rather than omitted, so callers that read
        // them do not error. Returns false for an out-of-range column index.
        if ($column < 0 || $column >= elephc_pdo_column_count($this->stmt)) {
            return false;
        }
        $_type = elephc_pdo_column_type($this->stmt, $column);
        $_native = "null";
        $_pdoType = 0;
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
            $_native = "blob";
            $_pdoType = 3;
        }
        // Prefer the column's DECLARED type (sqlite3_column_decltype), which PHP's
        // getColumnMeta reports as native_type; fall back to the value-type name for
        // an expression column (or a driver) with no declared type.
        $_decltype = elephc_pdo_column_decltype($this->stmt, $column);
        if ($_decltype !== "") {
            $_native = $_decltype;
        }
        return [
            "name" => elephc_pdo_column_name($this->stmt, $column),
            "native_type" => $_native,
            "pdo_type" => $_pdoType,
            "len" => 0,
            "precision" => 0,
            "flags" => [],
            "table" => "",
        ];
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

    public function __destruct() {
        // Finalize the prepared statement when the PDOStatement is collected. The
        // bridge ignores an unknown/already-finalized handle, so this is safe even
        // when the owning PDO connection was closed first (its close() already
        // finalized this statement).
        elephc_pdo_finalize($this->stmt);
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
            // descriptor for the connection's lifetime. Only closures and first-class
            // callables are supported (their value is a descriptor pointer); a string
            // or array callable is rejected at compile time by __elephc_callable_ptr.
            $this->udfCallbacks[$name] = $callback;
            $_descriptor = \__elephc_callable_ptr($callback);
            $_adapter = \__elephc_pdo_adapter_addr(0);
            return \elephc_pdo_create_collation($this->connectionId(), $name, $_descriptor, $_adapter) === 1;
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

        public function copyToArray(string $tableName, string $separator = "\t", string $nullAs = "\\N", ?string $fields = null): array {
            // Returns the table's rows, one array element per row (each keeping its
            // trailing newline, as PHP's copyToArray does). An empty result yields an
            // empty array; a transport error also yields an empty array, with the
            // connection's errorInfo set by the bridge.
            $_sql = "COPY " . $this->copyTarget($tableName, $fields) . " TO STDOUT"
                . $this->copyOptions($separator, $nullAs);
            $_raw = \elephc_pdo_copy_out($this->connectionId(), $_sql);
            if ($_raw === "") {
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
            $_sql = "COPY " . $this->copyTarget($tableName, $fields) . " TO STDOUT"
                . $this->copyOptions($separator, $nullAs);
            $_raw = \elephc_pdo_copy_out($this->connectionId(), $_sql);
            return \file_put_contents($filename, $_raw) !== false;
        }

        public function getNotify(int $fetchMode = 0, int $timeoutMilliseconds = 0): array {
            // Polls for a pending LISTEN/NOTIFY notification, returning it as a
            // numerically-indexed array [0=>channel, 1=>pid, 2=>payload], or an empty
            // array if none arrived within the timeout. Divergences from PHP: (1) an
            // empty array is returned rather than false for "no notification" (both
            // are falsy, so `while ($n = $db->getNotify())` still terminates); (2)
            // $fetchMode is accepted for signature compatibility but the result is
            // always the numerically-indexed (FETCH_NUM) shape — elephc's EIR array
            // backend cannot return a string-keyed array alongside an empty array from
            // one method, so the associative shapes are not produced.
            $_unused = $fetchMode;
            $_raw = \elephc_pdo_get_notify($this->connectionId(), $timeoutMilliseconds);
            if ($_raw === "") {
                return [];
            }
            // elephc's explode takes no limit argument, so a tab in the payload is
            // not preserved beyond its first segment (channel names and the pid never
            // contain tabs, and NOTIFY payloads virtually never do).
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
