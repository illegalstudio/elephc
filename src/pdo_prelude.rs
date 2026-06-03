//! Purpose:
//! The PDO (SQLite driver) standard-library surface, implemented in elephc-PHP.
//! Declares the `elephc_sqlite` bridge externs and the `PDO`, `PDOStatement`,
//! and `PDOException` classes, so the whole feature compiles through the normal
//! pipeline (classes, methods, exceptions, mixed arrays, C-ABI extern calls)
//! instead of bespoke intrinsics and hand-written assembly.
//!
//! Called from:
//! - `crate::pipeline::compile()` and the codegen test harness via `inject_if_used`,
//!   after include resolution and before name resolution.
//!
//! Key details:
//! - The prelude is only injected when the program references PDO, so non-PDO
//!   binaries never declare the `elephc_sqlite` externs and therefore never link
//!   `-lelephc_sqlite`.
//! - The prelude carries only declarations (extern block + classes), which are
//!   discovered position-independently, so it is prepended to user code without
//!   changing top-level execution order.
//! - Method-local variables are `$_`-prefixed because the checker resolves a
//!   method-body variable's type against top-level variables of the same name; a
//!   user global like `$stmt` (a `PDOStatement`) would otherwise clash with a
//!   plain method-local `$stmt`. The `$_` prefix also exempts them from the
//!   unused-variable warning.

use crate::parser::ast::{Program, Stmt};

/// The elephc-PHP source implementing PDO over the `elephc_sqlite` bridge.
///
/// Fetch-mode integers match PHP (`FETCH_ASSOC`=2, `FETCH_NUM`=3, `FETCH_BOTH`=4,
/// `FETCH_OBJ`=5); SQLite column-type integers match SQLite
/// (1=INTEGER, 2=FLOAT, 3=TEXT, 4=BLOB, 5=NULL). Method-default literals use the
/// numeric values directly to avoid const-in-default-value evaluation edge cases.
pub const PDO_PRELUDE_SRC: &str = r#"<?php

extern "elephc_sqlite" {
    function elephc_sqlite_open(string $dsn): int;
    function elephc_sqlite_last_open_error(): string;
    function elephc_sqlite_close(int $conn): void;
    function elephc_sqlite_exec(int $conn, string $sql): int;
    function elephc_sqlite_last_insert_id(int $conn): int;
    function elephc_sqlite_changes(int $conn): int;
    function elephc_sqlite_begin(int $conn): int;
    function elephc_sqlite_commit(int $conn): int;
    function elephc_sqlite_rollback(int $conn): int;
    function elephc_sqlite_errcode(int $conn): int;
    function elephc_sqlite_errmsg(int $conn): string;
    function elephc_sqlite_prepare(int $conn, string $sql): int;
    function elephc_sqlite_bind_parameter_index(int $stmt, string $name): int;
    function elephc_sqlite_bind_int(int $stmt, int $idx, int $val): int;
    function elephc_sqlite_bind_double(int $stmt, int $idx, float $val): int;
    function elephc_sqlite_bind_text(int $stmt, int $idx, string $val): int;
    function elephc_sqlite_bind_null(int $stmt, int $idx): int;
    function elephc_sqlite_reset(int $stmt): int;
    function elephc_sqlite_clear_bindings(int $stmt): int;
    function elephc_sqlite_step(int $stmt): int;
    function elephc_sqlite_column_count(int $stmt): int;
    function elephc_sqlite_column_name(int $stmt, int $i): string;
    function elephc_sqlite_column_type(int $stmt, int $i): int;
    function elephc_sqlite_column_int(int $stmt, int $i): int;
    function elephc_sqlite_column_double(int $stmt, int $i): float;
    function elephc_sqlite_column_text(int $stmt, int $i): string;
    function elephc_sqlite_finalize(int $stmt): int;
}

class PDOException extends RuntimeException {
}

class PDO {
    const FETCH_ASSOC = 2;
    const FETCH_NUM = 3;
    const FETCH_BOTH = 4;
    const FETCH_OBJ = 5;
    const PARAM_NULL = 0;
    const PARAM_INT = 1;
    const PARAM_STR = 2;
    const PARAM_BOOL = 5;
    const ATTR_ERRMODE = 3;
    const ERRMODE_SILENT = 0;
    const ERRMODE_WARNING = 1;
    const ERRMODE_EXCEPTION = 2;

    private int $conn;

    public function __construct(string $dsn, ?string $username = null, ?string $password = null, ?array $options = null) {
        // SQLite ignores credentials/options; reference these PDO-compatible
        // optional parameters so they are not flagged as unused.
        $_unused = [$username, $password, $options];
        $this->conn = elephc_sqlite_open($dsn);
        if ($this->conn < 0) {
            throw new PDOException(elephc_sqlite_last_open_error());
        }
    }

    public function exec(string $statement): int {
        $_affected = elephc_sqlite_exec($this->conn, $statement);
        if ($_affected < 0) {
            throw new PDOException(elephc_sqlite_errmsg($this->conn));
        }
        return $_affected;
    }

    public function prepare(string $query): PDOStatement {
        $_handle = elephc_sqlite_prepare($this->conn, $query);
        if ($_handle < 0) {
            throw new PDOException(elephc_sqlite_errmsg($this->conn));
        }
        return new PDOStatement($_handle, $this->conn);
    }

    public function query(string $query): PDOStatement {
        $_statement = $this->prepare($query);
        $_statement->execute();
        return $_statement;
    }

    public function lastInsertId(): string {
        return (string) elephc_sqlite_last_insert_id($this->conn);
    }

    public function beginTransaction(): bool {
        return elephc_sqlite_begin($this->conn) == 1;
    }

    public function commit(): bool {
        return elephc_sqlite_commit($this->conn) == 1;
    }

    public function rollBack(): bool {
        return elephc_sqlite_rollback($this->conn) == 1;
    }

    public function errorCode(): string {
        return (string) elephc_sqlite_errcode($this->conn);
    }

    public function errorInfo(): array {
        $_code = elephc_sqlite_errcode($this->conn);
        return [(string) $_code, $_code, elephc_sqlite_errmsg($this->conn)];
    }
}

class PDOStatement implements Iterator {
    private int $stmt;
    private int $conn;
    private int $fetchMode;
    private array $boundParams;
    private array $boundValues;
    private array $boundTypes;
    private $iterRow;
    private int $iterKey;

    public function __construct(int $handle, int $connection) {
        $this->stmt = $handle;
        $this->conn = $connection;
        $this->fetchMode = 4;
        $this->boundParams = [];
        $this->boundValues = [];
        $this->boundTypes = [];
        // Initialized to null (not false) so the inferred property type widens to
        // Mixed when rewind()/next() assign a fetched row; a bool initializer would
        // pin the type to bool and coerce stored rows away. rewind() always runs
        // before the first valid() check, so the initial value is never observed.
        $this->iterRow = null;
        $this->iterKey = 0;
    }

    public function setFetchMode(int $mode): bool {
        $this->fetchMode = $mode;
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
            $_slot = (int) elephc_sqlite_bind_parameter_index($this->stmt, (string) $parameter);
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
        elephc_sqlite_reset($this->stmt);
        elephc_sqlite_clear_bindings($this->stmt);
        // Apply bindValue()/bindParam() bindings recorded since construction.
        // Slots are already resolved to ints, so this loop never looks up an
        // index (keeping the body uniform across positional and named binds).
        $_count = count($this->boundParams);
        for ($_i = 0; $_i < $_count; $_i++) {
            $_slot = (int) $this->boundParams[$_i];
            $_value = $this->boundValues[$_i];
            $_btype = $this->boundTypes[$_i];
            if ($_btype == 0 || is_null($_value)) {
                elephc_sqlite_bind_null($this->stmt, $_slot);
            } elseif ($_btype == 1 || $_btype == 5) {
                elephc_sqlite_bind_int($this->stmt, $_slot, (int) $_value);
            } else {
                elephc_sqlite_bind_text($this->stmt, $_slot, (string) $_value);
            }
        }
        // Apply this call's parameter array (positional ? and named :name).
        if ($params !== null) {
            foreach ($params as $_key => $_pv) {
                if (is_int($_key)) {
                    $_idx = $_key + 1;
                } else {
                    $_idx = elephc_sqlite_bind_parameter_index($this->stmt, (string) $_key);
                }
                $_pslot = (int) $_idx;
                if (is_int($_pv)) {
                    elephc_sqlite_bind_int($this->stmt, $_pslot, (int) $_pv);
                } elseif (is_bool($_pv)) {
                    elephc_sqlite_bind_int($this->stmt, $_pslot, (int) $_pv);
                } elseif (is_float($_pv)) {
                    elephc_sqlite_bind_double($this->stmt, $_pslot, (float) $_pv);
                } elseif (is_null($_pv)) {
                    elephc_sqlite_bind_null($this->stmt, $_pslot);
                } else {
                    elephc_sqlite_bind_text($this->stmt, $_pslot, (string) $_pv);
                }
            }
        }
        // A statement with no result columns (INSERT/UPDATE/DELETE/DDL) is run
        // now; SELECT-style statements (column_count > 0) are stepped lazily by
        // fetch() so the first row is not consumed here.
        if (elephc_sqlite_column_count($this->stmt) == 0) {
            elephc_sqlite_step($this->stmt);
        }
        return true;
    }

    private function columnValue(int $index): mixed {
        $_type = elephc_sqlite_column_type($this->stmt, $index);
        if ($_type == 1) {
            return elephc_sqlite_column_int($this->stmt, $index);
        } elseif ($_type == 2) {
            return elephc_sqlite_column_double($this->stmt, $index);
        } elseif ($_type == 5) {
            return null;
        }
        return elephc_sqlite_column_text($this->stmt, $index);
    }

    public function fetch(int $mode = 0): mixed {
        if ($mode == 0) {
            $mode = $this->fetchMode;
        }
        $_rc = elephc_sqlite_step($this->stmt);
        if ($_rc != 1) {
            return false;
        }
        $_count = elephc_sqlite_column_count($this->stmt);
        if ($mode == 5) {
            // FETCH_OBJ: build the associative row, then round-trip through JSON
            // so json_decode yields a stdClass with one property per column.
            // (elephc does not yet support dynamic property assignment, so this
            // is how the object is materialized; numeric column names degrade to
            // an array, matching how json_decode treats list-shaped objects.)
            $_assoc = [];
            for ($_i = 0; $_i < $_count; $_i++) {
                $_name = elephc_sqlite_column_name($this->stmt, $_i);
                $_assoc[$_name] = $this->columnValue($_i);
            }
            return json_decode(json_encode($_assoc));
        }
        $_row = [];
        for ($_i = 0; $_i < $_count; $_i++) {
            $_value = $this->columnValue($_i);
            if ($mode == 3) {
                $_row[$_i] = $_value;
            } elseif ($mode == 2) {
                $_name = elephc_sqlite_column_name($this->stmt, $_i);
                $_row[$_name] = $_value;
            } else {
                $_name = elephc_sqlite_column_name($this->stmt, $_i);
                $_row[$_i] = $_value;
                $_row[$_name] = $_value;
            }
        }
        return $_row;
    }

    public function fetchAll(int $mode = 0): array {
        if ($mode == 0) {
            $mode = $this->fetchMode;
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

    public function fetchColumn(int $column = 0): mixed {
        $_rc = elephc_sqlite_step($this->stmt);
        if ($_rc != 1) {
            return false;
        }
        return $this->columnValue($column);
    }

    public function rowCount(): int {
        return elephc_sqlite_changes($this->conn);
    }

    public function columnCount(): int {
        return elephc_sqlite_column_count($this->stmt);
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
}
"#;

/// Returns whether the resolved program references PDO (so the prelude must be
/// injected). Sound by construction: any real `PDO`/`PDOStatement`/`PDOException`
/// reference appears in a statement's `Debug` form. A false positive from a
/// `"PDO"` string literal only over-links harmlessly. Short-circuits on the first
/// matching top-level statement.
pub fn program_uses_pdo(program: &[Stmt]) -> bool {
    program
        .iter()
        .any(|stmt| format!("{:?}", stmt).contains("PDO"))
}

/// Prepends the PDO prelude statements to `program` when it references PDO, so the
/// classes and `elephc_sqlite` externs compile through the normal pipeline only
/// for PDO-using programs. The prelude carries only declarations (extern block +
/// classes), which are hoisted, so prepending them ahead of user code does not
/// change top-level execution order. The prelude is static and tested, so a
/// tokenize/parse failure is a compiler bug and panics rather than silently
/// degrading.
pub fn inject_if_used(program: Program) -> Program {
    if !program_uses_pdo(&program) {
        return program;
    }
    let tokens = crate::lexer::tokenize(PDO_PRELUDE_SRC).expect("PDO prelude must tokenize");
    let mut combined = crate::parser::parse(&tokens).expect("PDO prelude must parse");
    combined.extend(program);
    combined
}
