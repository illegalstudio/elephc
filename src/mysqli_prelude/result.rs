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
//! - `$type` records MYSQLI_STORE_RESULT / MYSQLI_USE_RESULT. A USE_RESULT
//!   result streams its rows from the server one fetch at a time (the link
//!   stays busy until the set is exhausted or freed); num_rows reads 0 until
//!   exhausted and data_seek / mysqli_num_rows throw, like mysqlnd.
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
    public int|string $num_rows = 0;
    // MYSQLI_STORE_RESULT (0) or MYSQLI_USE_RESULT (1). A USE_RESULT result
    // reads its rows from the server on demand (num_rows reads 0 until the
    // last row was fetched; data_seek / mysqli_num_rows throw, like mysqlnd).
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

    // MYSQLI_USE_RESULT: the live bridge statement the rows are read from, one
    // fetch at a time (-1 for a buffered result), its next step verdict, the
    // owning link (notified when the set is exhausted or freed), the
    // MYSQLI_OPT_INT_AND_FLOAT_NATIVE conversion per column, and the rows read.
    private int $streamStmt = -1;
    private int $streamStep = 0;
    private ?mysqli $streamLink = null;
    private array $streamKinds = [];
    private int $streamRows = 0;

    // mysqlnd statistics: the link the rows are counted on (null: not counted),
    // whether they arrived in the binary protocol (a statement's get_result()),
    // the per-column statistic each decoded field charges, and whether the
    // result was already freed (counted once).
    private ?mysqli $statLink = null;
    private bool $statBinary = false;
    private array $statKeys = [];
    private bool $statFreed = false;

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
        $_placeholder = new mysqli();
        $_placeholder->__elephcDetachHandle();
        $_result = new mysqli_result($_placeholder);
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

    // Internal factory behind MYSQLI_USE_RESULT: a result that streams the
    // current set of $stmt from the server (its first step verdict is $firstStep).
    private static function __elephcFromStream(mysqli $link, int $stmt, int $firstStep, array $names, array $tables, array $nativeTypes, array $flags, array $lens, array $decimals, array $kinds): mysqli_result {
        mysqli_result::$fromFactory = true;
        $_placeholder = new mysqli();
        $_placeholder->__elephcDetachHandle();
        $_result = new mysqli_result($_placeholder);
        mysqli_result::$fromFactory = false;
        $_result->names = $names;
        $_result->metaTables = $tables;
        $_result->metaNativeTypes = $nativeTypes;
        $_result->metaFlags = $flags;
        $_result->metaLens = $lens;
        $_result->metaDecimals = $decimals;
        $_result->field_count = count($names);
        $_result->type = 1;
        $_result->num_rows = 0;
        $_result->streamStmt = $stmt;
        $_result->streamStep = $firstStep;
        $_result->streamLink = $link;
        $_result->streamKinds = $kinds;
        $_result->statLink = $link;
        return $_result;
    }

    // Copies this result into $target (the public constructor's pickup of a
    // pending result) and hands over a live stream; a method receiver, so the
    // narrowed source never needs a typed-property read.
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
        $target->streamStmt = $this->streamStmt;
        $target->streamStep = $this->streamStep;
        $target->streamLink = $this->streamLink;
        $target->streamKinds = $this->streamKinds;
        $this->streamStmt = -1;
        $this->streamLink = null;
    }

    // The conversion mysqlnd applies to a decoded cell of a column: 0 none,
    // 1 int / 2 float (MYSQLI_OPT_INT_AND_FLOAT_NATIVE on the text protocol),
    // 3 BIT as an integer (binary protocol, or text with the native option),
    // 4 BIT as its decimal string (plain text protocol), 5 ZEROFILL integer as
    // a string padded to the display width (binary protocol; the text protocol
    // already carries the padded string and keeps it even with the option).
    private static function __elephcCellKind(string $native, int $flags, bool $intFloatNative, bool $binary): int {
        if ($native === "BIT") {
            return ($binary || $intFloatNative) ? 3 : 4;
        }
        $_integer = $native === "TINY" || $native === "SHORT" || $native === "LONG"
            || $native === "LONGLONG" || $native === "INT24" || $native === "YEAR";
        if ($_integer && ($flags & 64) != 0) {
            return $binary ? 5 : 0;
        }
        if ($binary || !$intFloatNative) {
            return 0;
        }
        if ($_integer) {
            return 1;
        }
        if ($native === "FLOAT" || $native === "DOUBLE") {
            return 2;
        }
        return 0;
    }

    // Applies a __elephcCellKind conversion ($length: the column's display
    // width, used by the ZEROFILL padding).
    private static function __elephcConvertCell(mixed $value, int $kind, int $length): mixed {
        if ($value === null || $kind == 0) {
            return $value;
        }
        if ($kind == 1) {
            // An unsigned value past PHP_INT_MAX stays its decimal string.
            $_int = (int) $value;
            return (string) $_int === (string) $value ? $_int : $value;
        }
        if ($kind == 2) {
            return (float) $value;
        }
        if ($kind == 5) {
            return str_pad((string) $value, $length, "0", STR_PAD_LEFT);
        }
        // BIT: the big-endian bytes as an unsigned integer; one past
        // PHP_INT_MAX becomes its decimal string, like mysqlnd.
        $_bytes = (string) $value;
        $_bits = 0;
        for ($_i = 0; $_i < strlen($_bytes); $_i++) {
            $_bits = ($_bits << 8) | ord($_bytes[$_i]);
        }
        if ($_bits < 0) {
            $_half = ($_bits >> 1) & PHP_INT_MAX;
            $_tens = intdiv($_half, 5);
            $_unit = 2 * ($_half - $_tens * 5) + ($_bits & 1);
            return (string) $_tens . (string) $_unit;
        }
        return $kind == 3 ? $_bits : (string) $_bits;
    }

    // Has $link count this buffered result's statistics ($binary: a statement's
    // get_result(), decoded from the binary protocol).
    private function __elephcCountFor(mysqli $link, bool $binary): void {
        $this->statLink = $link;
        $this->statBinary = $binary;
    }

    // The statistic mysqlnd charges for one decoded field of column $index:
    // php-src's per-wire-type tables read through its (shifted) statistics
    // names — e.g. a binary BIGINT lands in proto_binary_fetched_decimal.
    private function statKey(int $index): string {
        if (count($this->statKeys) == 0) {
            $_text = [
            "DECIMAL" => "proto_text_fetched_decimal",
            "TINY" => "proto_text_fetched_tinyint",
            "SHORT" => "proto_text_fetched_short",
            "LONG" => "proto_text_fetched_int",
            "FLOAT" => "proto_text_fetched_float",
            "DOUBLE" => "proto_text_fetched_double",
            "NULL" => "proto_text_fetched_null",
            "TIMESTAMP" => "proto_text_fetched_timestamp",
            "LONGLONG" => "proto_text_fetched_bigint",
            "INT24" => "proto_text_fetched_int24",
            "DATE" => "proto_text_fetched_date",
            "TIME" => "proto_text_fetched_time",
            "DATETIME" => "proto_text_fetched_datetime",
            "YEAR" => "proto_text_fetched_year",
            "NEWDATE" => "proto_text_fetched_date",
            "VARCHAR" => "proto_text_fetched_string",
            "BIT" => "proto_text_fetched_bit",
            "NEWDECIMAL" => "proto_text_fetched_decimal",
            "ENUM" => "proto_text_fetched_set",
            "SET" => "proto_text_fetched_geometry",
            "JSON" => "proto_text_fetched_blob",
            "TINY_BLOB" => "proto_text_fetched_enum",
            "MEDIUM_BLOB" => "proto_text_fetched_enum",
            "LONG_BLOB" => "proto_text_fetched_enum",
            "BLOB" => "proto_text_fetched_enum",
            "VAR_STRING" => "proto_text_fetched_string",
            "STRING" => "proto_text_fetched_string",
            "GEOMETRY" => "proto_text_fetched_other",
            ];
            $_binary = [
            "DECIMAL" => "proto_binary_fetched_float",
            "TINY" => "proto_binary_fetched_short",
            "SHORT" => "proto_binary_fetched_int24",
            "LONG" => "proto_binary_fetched_bigint",
            "FLOAT" => "proto_binary_fetched_double",
            "DOUBLE" => "proto_binary_fetched_date",
            "NULL" => "proto_binary_fetched_bit",
            "TIMESTAMP" => "proto_binary_fetched_string",
            "LONGLONG" => "proto_binary_fetched_decimal",
            "INT24" => "proto_binary_fetched_int",
            "DATE" => "proto_binary_fetched_year",
            "TIME" => "proto_binary_fetched_datetime",
            "DATETIME" => "proto_binary_fetched_timestamp",
            "YEAR" => "proto_binary_fetched_time",
            "NEWDATE" => "proto_binary_fetched_year",
            "VARCHAR" => "proto_binary_fetched_json",
            "BIT" => "proto_binary_fetched_tinyint",
            "NEWDECIMAL" => "proto_binary_fetched_float",
            "ENUM" => "proto_binary_fetched_enum",
            "SET" => "proto_binary_fetched_set",
            "TINY_BLOB" => "proto_binary_fetched_blob",
            "MEDIUM_BLOB" => "proto_binary_fetched_blob",
            "LONG_BLOB" => "proto_binary_fetched_blob",
            "BLOB" => "proto_binary_fetched_blob",
            "VAR_STRING" => "proto_binary_fetched_json",
            "STRING" => "proto_binary_fetched_json",
            "GEOMETRY" => "proto_binary_fetched_geometry",
            ];
            for ($_i = 0; $_i < $this->field_count; $_i++) {
                $_native = (string) $this->metaNativeTypes[$_i];
                $this->statKeys[] = $this->statBinary
                    ? ($_binary[$_native] ?? "proto_binary_fetched_other")
                    : ($_text[$_native] ?? "proto_binary_fetched_null");
            }
        }
        return (string) $this->statKeys[$index];
    }

    // Bytes mysqlnd adds to bytes_received_real_data_* for one decoded field:
    // the text field's length; in the binary protocol a string's length or the
    // fixed width the value occupied on the wire.
    private function statBytes(int $index, mixed $value): int {
        $_native = (string) $this->metaNativeTypes[$index];
        if ($_native === "BIT") {
            $_bytes = intdiv((int) $this->metaLens[$index] + 7, 8);
            return $this->statBinary ? $_bytes + 1 : $_bytes;
        }
        if (!$this->statBinary || is_string($value)) {
            return strlen((string) $value);
        }
        if ($_native === "TINY") { return 1; }
        if ($_native === "SHORT" || $_native === "YEAR") { return 2; }
        if ($_native === "INT24" || $_native === "LONG" || $_native === "FLOAT") { return 4; }
        if ($_native === "LONGLONG" || $_native === "DOUBLE") { return 8; }
        return strlen((string) $value);
    }

    // mysqlnd's per-row accounting when a row is handed to the program: the
    // client-side row counter (buffered: process-wide only) and, per decoded
    // field, its type statistic and data bytes (a NULL text field charges
    // nothing; a NULL binary field its null statistic).
    private function countRow(array $row, bool $buffered): void {
        $_link = $this->statLink;
        if ($_link === null) {
            return;
        }
        if ($buffered) {
            $_link->__elephcStat($this->statBinary ? "rows_fetched_from_client_ps_buffered" : "rows_fetched_from_client_normal_buffered", 1, false);
        } else {
            $_link->__elephcStat("rows_fetched_from_server_normal", 1, true);
            $_link->__elephcStat("rows_fetched_from_client_normal_unbuffered", 1, true);
        }
        $_bytes = 0;
        for ($_i = 0; $_i < $this->field_count; $_i++) {
            $_value = $row[$_i];
            if ($_value === null) {
                if ($this->statBinary) {
                    $_link->__elephcStat("proto_binary_fetched_bit", 1, true);
                }
                continue;
            }
            $_link->__elephcStat($this->statKey($_i), 1, true);
            $_bytes = $_bytes + $this->statBytes($_i, $_value);
        }
        $_link->__elephcStat($this->statBinary ? "bytes_received_real_data_ps" : "bytes_received_real_data_normal", $_bytes, true);
    }

    // mysqli_result free (close / free / free_result / destruction): php
    // always reports it as explicit.
    private function countFree(): void {
        $_link = $this->statLink;
        if ($_link === null || $this->statFreed) {
            return;
        }
        $this->statFreed = true;
        $_link->__elephcStat("explicit_free_result", 1, true);
    }

    public function fetch_row(): ?array {
        if ($this->streamStmt >= 0 || $this->type == 1) {
            return $this->streamRow();
        }
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
        $this->countRow($_row, true);
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
        // php rejects an abstract class before touching the result set.
        $_isStd = strtolower($class) === "stdclass";
        if (!$_isStd && __elephc_class_is_abstract($class)) {
            throw new Error("Class " . $class . " cannot be instantiated");
        }
        $_row = $this->fetch_row();
        if ($_row === null) {
            return null;
        }
        // Two straight-line returns rather than one reassigned local (the
        // dynamic-new result is Mixed; see the PDO prelude's hydrateClassOrStd).
        if ($_isStd) {
            $_std = new stdClass();
            for ($_i = 0; $_i < $this->field_count; $_i++) {
                $_n = (string) $this->names[$_i];
                $_std->{$_n} = $_row[$_i];
            }
            return $_std;
        }
        // Like php-src, the columns are assigned BEFORE the constructor runs, so
        // a constructor sees (and may overwrite) the fetched values.
        $_object = $this->assignRow(__elephc_new_without_constructor($class), $_row);
        if (__elephc_class_has_constructor($class)) {
            call_user_func_array([$_object, "__construct"], $constructor_args);
        } elseif (count($constructor_args) != 0) {
            throw new ValueError("mysqli_result::fetch_object(): Argument #2 (\$constructor_args) must be empty when the specified class (" . $class . ") does not have a constructor");
        }
        return $_object;
    }

    // Writes one fetched row onto an object as properties (declared slots first,
    // then dynamic ones), the order php-src's zend_merge_properties uses.
    private function assignRow(mixed $object, array $row): mixed {
        for ($_i = 0; $_i < $this->field_count; $_i++) {
            $_n = (string) $this->names[$_i];
            $object->{$_n} = $row[$_i];
        }
        return $object;
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
        // An unbuffered set is skipped to its end (mysqlnd reads and discards
        // the remaining rows) so the link becomes usable again.
        $this->discardStream();
        $this->countFree();
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

    public function __destruct() {
        $this->discardStream();
        $this->countFree();
    }

    // Reads the next row of a MYSQLI_USE_RESULT set from the server; null —
    // and the link released — once the set is exhausted.
    private function streamRow(): ?array {
        // 2 = the next row was not read yet: like mysqlnd, read it only now.
        if ($this->streamStmt >= 0 && $this->streamStep == 2) {
            $this->streamStep = elephc_pdo_step($this->streamStmt);
        }
        if ($this->streamStmt >= 0 && $this->streamStep < 0) {
            // A failed read (timeout, lost connection) is an error, not the set's end:
            // like mysqli's fetch path, the link records it and reports it.
            $_link = $this->streamLink;
            if ($_link !== null) {
                $_link->__elephcAdoptBridgeError();
            }
            $this->endStream();
            $this->lengths = null;
            $this->num_rows = $this->streamRows;
            if ($_link !== null) {
                $_link->__elephcReportError();
            }
            return null;
        }
        if ($this->streamStmt < 0 || $this->streamStep != 1) {
            $this->endStream();
            $this->lengths = null;
            $this->num_rows = $this->streamRows;
            return null;
        }
        $_row = [];
        $_lengths = [];
        for ($_i = 0; $_i < $this->field_count; $_i++) {
            $_value = mysqli_result::__elephcConvertCell($this->columnValue($_i), (int) $this->streamKinds[$_i], (int) $this->metaLens[$_i]);
            $_row[] = $_value;
            if ($_value === null) {
                $_lengths[] = 0;
            } else {
                $_lengths[] = strlen((string) $_value);
            }
        }
        $this->streamRows = $this->streamRows + 1;
        // The following row (or the set's EOF) is read by the next fetch.
        $this->streamStep = 2;
        $this->lengths = $_lengths;
        $this->countRow($_row, false);
        return $_row;
    }

    // Skips the rest of a MYSQLI_USE_RESULT set and releases the link.
    private function discardStream(): void {
        if ($this->streamStmt < 0) {
            return;
        }
        // mysqlnd skip_result: a set not read to its EOF is flushed, and every
        // row still on the wire is read (and counted) and skipped.
        $_link = $this->statLink;
        if ($_link !== null && $this->streamStep != 0) {
            $_link->__elephcStat("flushed_normal_sets", 1, true);
        }
        while (true) {
            if ($this->streamStep == 2) {
                $this->streamStep = elephc_pdo_step($this->streamStmt);
            }
            if ($this->streamStep != 1) {
                break;
            }
            $this->streamRows = $this->streamRows + 1;
            $this->streamStep = 2;
            if ($_link !== null) {
                $_link->__elephcStat("rows_fetched_from_server_normal", 1, true);
                $_link->__elephcStat("rows_fetched_from_client_normal_unbuffered", 1, true);
                $_link->__elephcStat("rows_skipped_normal", 1, true);
            }
        }
        $this->endStream();
    }

    // Detaches from the live statement and tells the link its set is consumed.
    private function endStream(): void {
        if ($this->streamStmt < 0) {
            return;
        }
        $_stmt = $this->streamStmt;
        $this->streamStmt = -1;
        $_link = $this->streamLink;
        $this->streamLink = null;
        if ($_link !== null) {
            $_link->__elephcStreamFinished($_stmt, $this->streamRows);
        }
    }

    // Decodes one cell of the streamed row (int / float / null / length-counted
    // bytes), the same dispatch as the connection's columnValue.
    private function columnValue(int $index): mixed {
        $_type = elephc_pdo_column_type($this->streamStmt, $index);
        if ($_type == 1) {
            return elephc_pdo_column_int($this->streamStmt, $index);
        }
        if ($_type == 2) {
            return elephc_pdo_column_double($this->streamStmt, $index);
        }
        if ($_type == 5) {
            return null;
        }
        $_len = elephc_pdo_column_data_len($this->streamStmt, $index);
        if ($_len > 0) {
            return __elephc_ptr_read_string(elephc_pdo_column_data_ptr($this->streamStmt, $index), $_len);
        }
        return "";
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
        // mysqlnd adds NUM_FLAG client-side to every numeric wire type: those up
        // to INT24 (a TIMESTAMP only at the legacy display lengths 14 and 8) and
        // YEAR.
        $_flags = (int) $this->metaFlags[$index];
        $_length = (int) $this->metaLens[$index];
        if (($_type <= 9 && ($_type != 7 || $_length == 14 || $_length == 8)) || $_type == 13) {
            $_flags = $_flags | 32768;
        }
        $_field->{"flags"} = $_flags;
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
