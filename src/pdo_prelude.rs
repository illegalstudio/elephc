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
        $_stmt = new PDOStatement($_handle, $this->conn, $this->errMode);
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

    public function __construct(int $handle, int $connection, int $errMode = 2) {
        $this->stmt = $handle;
        $this->conn = $connection;
        $this->errMode = $errMode;
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

// PHP 8.4 driver-specific PDO subclasses. In PHP they are returned by
// PDO::connect() and can also be constructed directly; each inherits the full
// base PDO connection surface (constructor, exec/query/prepare, transactions,
// quoting) from \PDO. Only the shared base is provided here: driver-specific
// methods (e.g. Pdo\Sqlite::createFunction, Pdo\Mysql::getWarningCount,
// Pdo\Pgsql::escapeIdentifier) require callable/driver plumbing and are tracked
// as a follow-up.
//
// The classes are declared in a BLOCK-form namespace: a statement-form
// `namespace Pdo;` would apply to every statement that follows it, and because
// this prelude is prepended ahead of user code that would silently re-namespace
// the entire user program. The block keeps the `Pdo\` scope contained, leaving
// the appended user code in the global namespace. `extends \PDO` is
// fully-qualified so it binds to the global prelude PDO regardless of scope.
namespace Pdo {
    class Sqlite extends \PDO {}

    class Mysql extends \PDO {}

    class Pgsql extends \PDO {}
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
