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
}

class PDOException extends RuntimeException {
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
    const ATTR_ERRMODE = 3;
    const ATTR_PERSISTENT = 12;
    const ATTR_DRIVER_NAME = 16;
    const ERRMODE_SILENT = 0;
    const ERRMODE_WARNING = 1;
    const ERRMODE_EXCEPTION = 2;

    private int $conn;
    private int $errMode;
    private bool $persistent;
    private array $attributes;

    public function __construct(string $dsn, ?string $username = null, ?string $password = null, ?array $options = null) {
        $this->errMode = 2;
        $this->persistent = false;
        $this->attributes = [];
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
    }

    private function fail(string $message): void {
        // Apply the current error mode to a failed operation. EXCEPTION throws;
        // WARNING writes to stderr and lets the caller return its failure value;
        // SILENT is quiet and the caller returns its failure value.
        if ($this->errMode == 2) {
            throw new PDOException($message);
        }
        if ($this->errMode == 1) {
            fwrite(STDERR, "PDO error: " . $message . "\n");
        }
    }

    public function setAttribute(int $attribute, $value): bool {
        if ($attribute == 3) {
            $this->errMode = (int) $value;
        } elseif ($attribute == 12) {
            $this->persistent = (bool) $value;
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
        return new PDOStatement($_handle, $this->conn, $this->errMode);
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
        // The name is a sequence for PostgreSQL (`currval($name)`); SQLite
        // ignores it and returns the last rowid.
        return (string) elephc_pdo_last_insert_id($this->conn, $name ?? "");
    }

    public function beginTransaction(): bool {
        if (elephc_pdo_begin($this->conn) != 1) {
            $this->fail(elephc_pdo_errmsg($this->conn));
            return false;
        }
        return true;
    }

    public function commit(): bool {
        if (elephc_pdo_commit($this->conn) != 1) {
            $this->fail(elephc_pdo_errmsg($this->conn));
            return false;
        }
        return true;
    }

    public function rollBack(): bool {
        if (elephc_pdo_rollback($this->conn) != 1) {
            $this->fail(elephc_pdo_errmsg($this->conn));
            return false;
        }
        return true;
    }

    public function errorCode(): string {
        // The driver's native result code as a string. This is the native code,
        // not a 5-character SQLSTATE (see errorInfo()): no supported driver's
        // client library exposes SQLSTATEs here.
        return (string) elephc_pdo_errcode($this->conn);
    }

    public function errorInfo(): array {
        // PHP's errorInfo() is [SQLSTATE, driver-specific code, message]. The
        // client libraries used here do not surface real 5-character SQLSTATEs
        // (SQLite and MySQL expose native integer codes; the PostgreSQL client
        // surfaces only a message, reported as a generic code), so the first
        // element mirrors the native driver code as a string, not a true
        // SQLSTATE.
        $_code = elephc_pdo_errcode($this->conn);
        return [(string) $_code, $_code, elephc_pdo_errmsg($this->conn)];
    }

    public function quote(string $string, int $type = 2): string {
        // SQLite-style string-literal quoting for every driver: wrap in single
        // quotes and double any embedded single quote. The $type argument is
        // accepted for PHP signature compatibility but ignored. This is not
        // driver-aware (e.g. it does not apply MySQL backslash escaping), so
        // prefer prepared statements — the recommended path for all drivers.
        $_unused = $type;
        return "'" . str_replace("'", "''", $string) . "'";
    }

    public function __destruct() {
        // Release the bridge connection when the PDO object is collected. The
        // bridge finalizes the connection's remaining statements before closing,
        // and treats an already-closed handle as a no-op, so the order relative
        // to any surviving PDOStatement destructors does not matter.
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
    }

    private function fail(string $message): void {
        if ($this->errMode == 2) {
            throw new PDOException($message);
        }
        if ($this->errMode == 1) {
            fwrite(STDERR, "PDO error: " . $message . "\n");
        }
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
        if ($mode == 0) {
            $mode = $this->fetchMode;
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
        if ($mode == 7) {
            // FETCH_COLUMN: yield a single column's value as a scalar instead of a
            // row array. The column index defaults to 0 and is set via the second
            // argument to setFetchMode(PDO::FETCH_COLUMN, $col).
            return $this->columnValue($this->fetchColumn);
        }
        if ($mode == 5) {
            // FETCH_OBJ: materialize a real stdClass and assign each column as a
            // dynamic property, preserving numeric property names and binary data.
            return $this->assignColumns(new stdClass(), $_count);
        }
        if ($mode == 8) {
            if ($classOrObject !== null) {
                return $this->assignColumns(new $classOrObject(), $_count);
            }
            if ($this->fetchTarget !== null) {
                $_classTarget = $this->fetchTarget;
                return $this->assignColumns(new $_classTarget(), $_count);
            }
            return $this->assignColumns(new stdClass(), $_count);
        }
        if ($mode == 9) {
            if ($classOrObject !== null) {
                return $this->assignColumns($classOrObject, $_count);
            }
            if ($this->fetchTarget !== null) {
                return $this->assignColumns($this->fetchTarget, $_count);
            }
            return $this->assignColumns(new stdClass(), $_count);
        }
        if ($mode == 3) {
            $_numRow = [];
            for ($_i = 0; $_i < $_count; $_i++) {
                $_numRow[$_i] = $this->columnValue($_i);
            }
            return $_numRow;
        }
        if ($mode == 2) {
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
"#;

/// Prepends the PDO prelude statements to `program` when it references PDO, so the
/// classes and `elephc_pdo` externs compile through the normal pipeline only
/// for PDO-using programs. The prelude carries only declarations (extern block +
/// classes), which are hoisted, so prepending them ahead of user code does not
/// change top-level execution order. The prelude is static and tested, so a
/// tokenize/parse failure is a compiler bug and panics rather than silently
/// degrading.
pub fn inject_if_used(program: Program) -> Program {
    if !detect::program_uses_pdo(&program) {
        return program;
    }
    let tokens = crate::lexer::tokenize(PDO_PRELUDE_SRC).expect("PDO prelude must tokenize");
    let mut combined = crate::parser::parse(&tokens).expect("PDO prelude must parse");
    combined.extend(program);
    combined
}
