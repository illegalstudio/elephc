//! Purpose:
//! The `mysqli_result` class (plus its foreach iterator), as an elephc-PHP
//! source fragment (the parse-parity oracle for `build::result`). A result OWNS
//! its rows: every producer drains the bridge statement into the result before
//! handing it out, so a later query can never invalidate an earlier result.
//!
//! Called from:
//! - `crate::mysqli_prelude::fragments::source_for_version` (concatenated into
//!   the injected prelude).
//!
//! Key details:
//! - Rows are stored positionally; assoc/both shapes are derived from the
//!   captured column-name list at fetch time (duplicate names: last wins, like
//!   PHP). Field metadata is captured at drain time.
//! - The public constructor (`new mysqli_result($link, $mode)`) picks up the
//!   connection's pending result like `store_result()` / `use_result()`; the
//!   internal factory bypasses it through a static flag.
//! - `$type` records MYSQLI_STORE_RESULT / MYSQLI_USE_RESULT. Rows are always
//!   buffered, but a USE_RESULT result keeps mysqlnd's observable unbuffered
//!   rules (num_rows 0 until exhausted, data_seek / mysqli_num_rows throw).
//! - `fetch_column` is PHP 8.1+; its block is version-gated in `fragments.rs`.

/// The `mysqli_result` class and its iterator (fragment without a `<?php`
/// header).
pub(super) const SRC: &str = r##"
// -- mysqli buffered result surface --

class mysqli_result implements IteratorAggregate {
    public int $current_field = 0;
    public int $field_count = 0;
    // Column byte lengths of the row returned by the most recent fetch, or
    // null before the first fetch / after exhaustion (php-src's $lengths).
    public ?array $lengths = null;
    public int $num_rows = 0;
    // MYSQLI_STORE_RESULT (0) or MYSQLI_USE_RESULT (1): how the result was
    // requested. Rows are always buffered; a USE_RESULT result still behaves
    // like mysqlnd's unbuffered one where php can observe it (num_rows reads 0
    // until the last row was fetched, data_seek / mysqli_num_rows throw).
    public int $type = 0;

    // Set by the internal factory so `new mysqli_result($link)` inside it skips
    // the public constructor's pending-result pickup.
    private static bool $fromFactory = false;

    // Buffered cells and drain-time column metadata. Cells are stored FLAT in
    // row-major order (row * field_count + column) so every buffered value is
    // a Mixed scalar — a fetch builds a fresh row array.
    private array $cells = [];
    private array $names = [];
    private array $metaTables = [];
    private array $metaNativeTypes = [];
    private array $metaFlags = [];
    private array $metaLens = [];
    private array $metaDecimals = [];
    private int $rowCount = 0;
    private int $pos = 0;

    // php: `new mysqli_result($mysql, $mode)` takes over the connection's
    // pending result set exactly like store_result() / use_result().
    public function __construct(mysqli $mysql, int $result_mode = 0) {
        if (mysqli_result::$fromFactory) {
            return;
        }
        if ($mysql->__elephcConn() < 0) {
            throw new Error("mysqli object is not fully initialized");
        }
        if ($result_mode != 0 && $result_mode != 1) {
            throw new ValueError("mysqli_result::__construct(): Argument #2 (\$result_mode) must be either MYSQLI_STORE_RESULT or MYSQLI_USE_RESULT");
        }
        $_source = $result_mode == 1 ? $mysql->use_result() : $mysql->store_result();
        if ($_source === false) {
            return;
        }
        $_source->__elephcCopyInto($this, $result_mode);
    }

    // Internal factory used by the connection / statement drains (not part of
    // PHP's surface). `$cells` is the flat row-major cell list described above.
    // The constructor needs SOME mysqli; an unconnected placeholder is enough
    // because the factory flag makes it skip the pending-result pickup.
    private static function __elephcFromDrain(array $cells, int $rowCount, array $names, array $tables, array $nativeTypes, array $flags, array $lens, array $decimals): mysqli_result {
        mysqli_result::$fromFactory = true;
        $_result = new mysqli_result(new mysqli());
        mysqli_result::$fromFactory = false;
        $_result->cells = $cells;
        $_result->rowCount = $rowCount;
        $_result->names = $names;
        $_result->metaTables = $tables;
        $_result->metaNativeTypes = $nativeTypes;
        $_result->metaFlags = $flags;
        $_result->metaLens = $lens;
        $_result->metaDecimals = $decimals;
        $_result->num_rows = $rowCount;
        $_result->field_count = count($names);
        return $_result;
    }

    // Copies this buffered result into $target (the public constructor's
    // pickup of a pending result); a method receiver, so the narrowed source
    // never needs a typed-property read.
    private function __elephcCopyInto(mysqli_result $target, int $mode): void {
        $target->cells = $this->cells;
        $target->names = $this->names;
        $target->metaTables = $this->metaTables;
        $target->metaNativeTypes = $this->metaNativeTypes;
        $target->metaFlags = $this->metaFlags;
        $target->metaLens = $this->metaLens;
        $target->metaDecimals = $this->metaDecimals;
        $target->rowCount = $this->rowCount;
        $target->field_count = $this->field_count;
        $target->type = $mode;
        $target->num_rows = $mode == 1 ? 0 : $this->rowCount;
    }

    // Marks the result as STORE (0) or USE (1) mode once its producer knows.
    private function __elephcSetType(int $type): void {
        $this->type = $type;
        $this->num_rows = $type == 1 ? 0 : $this->rowCount;
    }

    public function fetch_row(): ?array {
        if ($this->pos >= $this->rowCount) {
            $this->lengths = null;
            if ($this->type == 1) {
                // Unbuffered semantics: the row count is known once exhausted.
                $this->num_rows = $this->rowCount;
            }
            return null;
        }
        $_base = $this->pos * $this->field_count;
        $this->pos = $this->pos + 1;
        $_row = [];
        $_lengths = [];
        for ($_i = 0; $_i < $this->field_count; $_i++) {
            $_value = $this->cells[$_base + $_i];
            $_row[] = $_value;
            if ($_value === null) {
                $_lengths[] = 0;
            } else {
                $_lengths[] = strlen((string) $_value);
            }
        }
        $this->lengths = $_lengths;
        return $_row;
    }

    public function fetch_assoc(): ?array {
        $_row = $this->fetch_row();
        if ($_row === null) {
            return null;
        }
        $_assoc = [];
        for ($_i = 0; $_i < $this->field_count; $_i++) {
            // Duplicate column names: last wins, matching PHP's assoc shape.
            $_assoc[(string) $this->names[$_i]] = $_row[$_i];
        }
        return $_assoc;
    }

    public function fetch_array(int $mode = 3): ?array {
        if ($mode != 1 && $mode != 2 && $mode != 3) {
            throw new ValueError("mysqli_result::fetch_array(): Argument #1 (\$mode) must be one of MYSQLI_NUM, MYSQLI_ASSOC, or MYSQLI_BOTH");
        }
        $_row = $this->fetch_row();
        if ($_row === null) {
            return null;
        }
        if ($mode == 2) {
            return $_row;
        }
        $_out = [];
        if ($mode == 3) {
            for ($_i = 0; $_i < $this->field_count; $_i++) {
                $_out[$_i] = $_row[$_i];
            }
        }
        for ($_i = 0; $_i < $this->field_count; $_i++) {
            $_out[(string) $this->names[$_i]] = $_row[$_i];
        }
        return $_out;
    }

    public function fetch_object(string $class = "stdClass", array $constructor_args = []): mixed {
        $_row = $this->fetch_row();
        if ($_row === null) {
            return null;
        }
        // Two straight-line returns rather than one reassigned local (the
        // dynamic-new result is Mixed; see the PDO prelude's hydrateClassOrStd).
        if (strtolower($class) === "stdclass") {
            $_std = new stdClass();
            for ($_i = 0; $_i < $this->field_count; $_i++) {
                $_n = (string) $this->names[$_i];
                $_std->{$_n} = $_row[$_i];
            }
            return $_std;
        }
        // Documented divergence: php-src assigns properties BEFORE calling the
        // constructor; elephc constructs first (PDO::FETCH_PROPS_LATE order).
        $_object = new $class(...$constructor_args);
        for ($_i = 0; $_i < $this->field_count; $_i++) {
            $_n = (string) $this->names[$_i];
            $_object->{$_n} = $_row[$_i];
        }
        return $_object;
    }

    public function fetch_all(int $mode = 2): array {
        if ($mode != 1 && $mode != 2 && $mode != 3) {
            throw new ValueError("mysqli_result::fetch_all(): Argument #1 (\$mode) must be one of MYSQLI_NUM, MYSQLI_ASSOC, or MYSQLI_BOTH");
        }
        $_all = [];
        // A fresh `$_row` each iteration, narrowed by the `=== null` break.
        while (true) {
            $_row = $this->fetch_array($mode);
            if ($_row === null) {
                break;
            }
            $_all[] = $_row;
        }
        return $_all;
    }

    // -- elephc PHP >= 8.1 mysqli fetch_column begin --
    public function fetch_column(int $column = 0): mixed {
        if ($column < 0) {
            throw new ValueError("mysqli_result::fetch_column(): Argument #1 (\$column) must be greater than or equal to 0");
        }
        if ($column >= $this->field_count) {
            throw new ValueError("mysqli_result::fetch_column(): Argument #1 (\$column) must be less than the number of fields for this result set");
        }
        $_row = $this->fetch_row();
        if ($_row === null) {
            return false;
        }
        return $_row[$column];
    }
    // -- elephc PHP >= 8.1 mysqli fetch_column end --

    public function data_seek(int $offset): bool {
        if ($offset < 0) {
            throw new ValueError("mysqli_result::data_seek(): Argument #1 (\$offset) must be greater than or equal to 0");
        }
        if ($this->type == 1) {
            throw new Error("mysqli_result::data_seek() cannot be used in MYSQLI_USE_RESULT mode");
        }
        if ($offset >= $this->rowCount) {
            return false;
        }
        $this->pos = $offset;
        return true;
    }

    public function fetch_field_direct(int $index): mixed {
        if ($index < 0) {
            throw new ValueError("mysqli_result::fetch_field_direct(): Argument #1 (\$index) must be greater than or equal to 0");
        }
        if ($index >= $this->field_count) {
            throw new ValueError("mysqli_result::fetch_field_direct(): Argument #1 (\$index) must be less than the number of fields for this result set");
        }
        return $this->fieldObject($index);
    }

    public function fetch_field(): mixed {
        if ($this->current_field >= $this->field_count) {
            return false;
        }
        $_field = $this->fieldObject($this->current_field);
        $this->current_field = $this->current_field + 1;
        return $_field;
    }

    public function fetch_fields(): array {
        $_fields = [];
        for ($_i = 0; $_i < $this->field_count; $_i++) {
            $_fields[] = $this->fieldObject($_i);
        }
        return $_fields;
    }

    public function field_seek(int $index): bool {
        // Moves the fetch_field() cursor (php 8: true, ValueError out of range).
        if ($index < 0) {
            throw new ValueError("mysqli_result::field_seek(): Argument #1 (\$index) must be greater than or equal to 0");
        }
        if ($index >= $this->field_count) {
            throw new ValueError("mysqli_result::field_seek(): Argument #1 (\$index) must be less than the number of fields for this result set");
        }
        $this->current_field = $index;
        return true;
    }

    public function close(): void {
        // Drop the buffered cells; further fetches see an exhausted cursor.
        $this->cells = [];
        $this->rowCount = 0;
        $this->num_rows = 0;
        $this->pos = 0;
        $this->lengths = null;
    }

    public function free(): void {
        $this->close();
    }

    public function free_result(): void {
        $this->close();
    }

    public function getIterator(): Iterator {
        return new __ElephcMysqliResultIterator($this);
    }

    // Builds php's field-description object for column $index. Metadata the
    // bridge does not expose (def, db, charsetnr) reads "" / 0; orgname and
    // orgtable mirror name and table; max_length is always 0, like mysqlnd.
    private function fieldObject(int $index): stdClass {
        $_type = $this->mysqliTypeFromNative((string) $this->metaNativeTypes[$index]);
        $_field = new stdClass();
        $_field->{"name"} = (string) $this->names[$index];
        $_field->{"orgname"} = (string) $this->names[$index];
        $_field->{"table"} = (string) $this->metaTables[$index];
        $_field->{"orgtable"} = (string) $this->metaTables[$index];
        $_field->{"def"} = "";
        $_field->{"db"} = "";
        $_field->{"catalog"} = "def";
        $_field->{"max_length"} = 0;
        $_field->{"length"} = (int) $this->metaLens[$index];
        $_field->{"charsetnr"} = 0;
        $_field->{"flags"} = (int) $this->metaFlags[$index];
        $_field->{"type"} = $_type;
        // The bridge's column_precision is MySQL's own wire "decimals" byte.
        $_field->{"decimals"} = (int) $this->metaDecimals[$index];
        return $_field;
    }

    // php-src type_to_name_native reversed: the bridge reports MySQL's own
    // wire-type names; map them onto the MYSQLI_TYPE_* integers with
    // MYSQLI_TYPE_STRING as the default for anything unrecognized.
    private function mysqliTypeFromNative(string $native): int {
        if ($native === "DECIMAL") { return 0; }
        if ($native === "TINY") { return 1; }
        if ($native === "SHORT") { return 2; }
        if ($native === "LONG") { return 3; }
        if ($native === "FLOAT") { return 4; }
        if ($native === "DOUBLE") { return 5; }
        if ($native === "NULL") { return 6; }
        if ($native === "TIMESTAMP") { return 7; }
        if ($native === "LONGLONG") { return 8; }
        if ($native === "INT24") { return 9; }
        if ($native === "DATE") { return 10; }
        if ($native === "TIME") { return 11; }
        if ($native === "DATETIME") { return 12; }
        if ($native === "YEAR") { return 13; }
        if ($native === "NEWDATE") { return 14; }
        if ($native === "VARCHAR") { return 15; }
        if ($native === "BIT") { return 16; }
        if ($native === "VECTOR") { return 242; }
        if ($native === "JSON") { return 245; }
        if ($native === "NEWDECIMAL") { return 246; }
        if ($native === "ENUM") { return 247; }
        if ($native === "SET") { return 248; }
        if ($native === "TINY_BLOB") { return 249; }
        if ($native === "MEDIUM_BLOB") { return 250; }
        if ($native === "LONG_BLOB") { return 251; }
        if ($native === "BLOB") { return 252; }
        if ($native === "VAR_STRING") { return 253; }
        if ($native === "STRING") { return 254; }
        if ($native === "GEOMETRY") { return 255; }
        return 254;
    }
}

// Iterator behind mysqli_result::getIterator(): assoc rows keyed by position,
// starting from row 0 regardless of the fetch cursor (rewind() seeks).
final class __ElephcMysqliResultIterator implements Iterator {
    private mysqli_result $result;
    private mixed $row;
    private int $position;

    public function __construct(mysqli_result $result) {
        $this->result = $result;
        $this->row = null;
        $this->position = 0;
    }

    public function rewind(): void {
        if ($this->result->type == 0) {
            $this->result->data_seek(0);
        }
        $this->row = $this->result->fetch_assoc();
        $this->position = 0;
    }

    public function valid(): bool {
        return $this->row !== null;
    }

    public function current(): mixed {
        return $this->row;
    }

    public function key(): mixed {
        return $this->position;
    }

    public function next(): void {
        $this->row = $this->result->fetch_assoc();
        $this->position = $this->position + 1;
    }
}
"##;
