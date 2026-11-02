//! Purpose:
//! The `mysqli_stmt` class, as an elephc-PHP source fragment: native prepares
//! over `elephc_pdo_prepare(…, 0)`, parameter binding, execute/step, and
//! `get_result` draining into an independent `mysqli_result`.
//!
//! Called from:
//! - `crate::mysqli_prelude::fragments::source_for_version` (concatenated into
//!   the injected prelude).
//!
//! Key details:
//! - `bind_param(string $types, mixed &...$vars)` keeps PHP's signature
//!   (literals are rejected, the variable count is validated against the type
//!   string), but the variable VALUES are captured at bind time — a documented
//!   divergence: elephc cannot alias caller variables past the call (variadic
//!   by-ref elements alias only inside the callee frame, and per-variable
//!   closure captures need fixed parameter names, whose defaulted-by-ref
//!   lowering the backend does not support). Re-executing with fresh values
//!   means re-calling bind_param or using `execute($params)`.
//! - `bind_result` / `fetch` are deliberately NOT declared: writing a fetched
//!   row back into caller variables after the binding call is exactly the
//!   post-call aliasing elephc cannot express, and a silently inert binding
//!   would be worse than an honest "undefined method". `get_result()` +
//!   `mysqli_result` fetches cover row reading; `method_exists` stays honest.
//! - `execute` steps once to learn whether a result set exists; `get_result`
//!   drains the rest and resets the statement so it can be re-executed;
//!   `store_result` consumes the pending rows so `num_rows` is valid.

/// The `mysqli_stmt` class (fragment without a `<?php` header).
pub(super) const SRC: &str = r##"
// -- mysqli prepared-statement surface --

class mysqli_stmt {
    public int $affected_rows = 0;
    public int $errno = 0;
    public string $error = "";
    public int $field_count = 0;
    public int $insert_id = 0;
    public int $num_rows = 0;
    public int $param_count = 0;
    public string $sqlstate = "00000";
    public array $error_list = [];

    // Bridge handles: the prepared-statement handle and the owning connection.
    private int $stmt = -1;
    private int $conn = -1;
    private ?mysqli $link = null;

    // bind_param state: the type string and the values captured at bind time.
    private string $bindTypes = "";
    private array $boundParams = [];

    // Cursor state: execute() pre-steps once; get_result/store_result drain.
    private bool $executedOnce = false;
    private bool $hasPending = false;
    private int $pendingStep = 0;

    // Internal factory used by mysqli::prepare (a user never constructs
    // mysqli_stmt directly in the v1 subset).
    private static function __elephcFromPrepare(mysqli $link, int $conn, int $stmt, string $query): mysqli_stmt {
        $_statement = new mysqli_stmt();
        $_statement->link = $link;
        $_statement->conn = $conn;
        $_statement->stmt = $stmt;
        // The bridge reports the server's exact parameter marker count off the
        // prepared statement (no client-side `?`-scanning, which used to diverge
        // from the multi-statement scanner on `--` comments / backslash rules).
        $_statement->param_count = elephc_pdo_mysql_param_count($stmt);
        return $_statement;
    }

    // Internal factory for the two-step mysqli::stmt_init() + prepare() form:
    // an unprepared statement bound to a connection, ready for prepare().
    // Private like __elephcFromPrepare — the checker's mysqli friend channel
    // exposes it to mysqli::stmt_init only.
    private static function __elephcInit(mysqli $link, int $conn): mysqli_stmt {
        $_statement = new mysqli_stmt();
        $_statement->link = $link;
        $_statement->conn = $conn;
        return $_statement;
    }

    public function prepare(string $query): bool {
        if ($this->conn < 0) {
            $this->syntheticFailure(2006, "mysqli_stmt object is not associated with a connection", "HY000");
            return false;
        }
        if ($query === "") {
            throw new ValueError("mysqli_stmt::prepare(): Argument #1 (\$query) cannot be empty");
        }
        if (!$this->requireLinkNotBusy()) {
            return false;
        }
        if ($this->stmt >= 0) {
            elephc_pdo_finalize($this->stmt);
            $this->stmt = -1;
        }
        $_handle = elephc_pdo_prepare($this->conn, $query, 0);
        if ($_handle < 0) {
            // stmt is still -1, so opFailed reads the connection's error state
            // (its stmt-errno fallback), which is where a prepare error lives.
            $this->opFailed();
            return false;
        }
        $this->stmt = $_handle;
        $this->param_count = elephc_pdo_mysql_param_count($_handle);
        $this->executedOnce = false;
        $this->hasPending = false;
        $this->clearError();
        return true;
    }

    // Guards the two-step prepare() and execute() while the LINK has
    // unconsumed results (a multi_query batch or a real_query result):
    // php-src raises CR_COMMANDS_OUT_OF_SYNC (2014) on the statement there —
    // sending COM_STMT_PREPARE / COM_STMT_EXECUTE on a busy connection would
    // corrupt the pending batch. One-shot mysqli::prepare() has its own
    // connection-level guard; this is the statement-side twin, reading the
    // link's private state through the checker's mysqli friend channel.
    private function requireLinkNotBusy(): bool {
        $_link = $this->link;
        if ($_link !== null && $_link->__elephcHasPendingResults()) {
            $this->syntheticFailure(2014, "Commands out of sync; you can't run this command now", "HY000");
            return false;
        }
        return true;
    }

    public function free_result(): void {
        // Discards a pending buffered result so the statement can be re-executed
        // (php frees the client-side buffer). We buffer per-statement in the
        // bridge, so draining/resetting the cursor is the equivalent.
        if ($this->hasPending && $this->stmt >= 0) {
            elephc_pdo_reset($this->stmt);
        }
        $this->hasPending = false;
    }

    public function bind_param(string $types, mixed &...$vars): bool {
        // Variadic by-ref keeps PHP's signature contract (a literal argument
        // is rejected with "must be passed a variable"); the values are
        // captured HERE — see the module preamble for the documented
        // divergence from PHP's read-at-execute reference semantics.
        $_values = [];
        $_given = count($vars);
        for ($_i = 0; $_i < $_given; $_i++) {
            $_values[] = $vars[$_i];
        }
        return $this->__elephcBindParamValues($types, $_values);
    }

    // Shared validation + snapshot behind bind_param (the procedural
    // mysqli_stmt_bind_param alias forwards through bind_param itself).
    // Private: not part of PHP's mysqli surface.
    private function __elephcBindParamValues(string $types, array $values): bool {
        $_want = strlen($types);
        if ($_want == 0) {
            throw new ValueError("mysqli_stmt::bind_param(): Argument #1 (\$types) cannot be empty");
        }
        for ($_i = 0; $_i < $_want; $_i++) {
            $_char = substr($types, $_i, 1);
            if ($_char !== "i" && $_char !== "d" && $_char !== "s" && $_char !== "b") {
                throw new ValueError("mysqli_stmt::bind_param(): Argument #1 (\$types) must only contain the \"b\", \"d\", \"i\", \"s\" type specifiers");
            }
        }
        if (count($values) != $_want) {
            $this->syntheticFailure(2031, "The number of variables must match the number of parameters in the prepared statement", "HY000");
            return false;
        }
        $this->bindTypes = $types;
        $this->boundParams = $values;
        return true;
    }

    public function execute(?array $params = null): bool {
        if ($this->stmt < 0) {
            $this->syntheticFailure(2006, "mysqli_stmt object is already closed", "HY000");
            return false;
        }
        if (!$this->requireLinkNotBusy()) {
            return false;
        }
        if ($this->executedOnce) {
            // Re-execution: rewind the server-side cursor and drop old binds.
            elephc_pdo_reset($this->stmt);
            elephc_pdo_clear_bindings($this->stmt);
        }
        $this->executedOnce = true;
        $this->hasPending = false;
        $this->pendingStep = 0;
        if ($params !== null) {
            // PHP 8.1+: execute's own $params bind as strings, in order.
            $_given = count($params);
            if ($_given != $this->param_count) {
                $this->syntheticFailure(2031, "mysqli_stmt::execute(): Argument #1 (\$params) must consist of exactly " . $this->param_count . " elements, " . $_given . " given", "HY000");
                return false;
            }
            for ($_i = 0; $_i < $_given; $_i++) {
                $_value = $params[$_i];
                if ($_value === null) {
                    elephc_pdo_bind_null($this->stmt, $_i + 1);
                } else {
                    $_text = (string) $_value;
                    elephc_pdo_bind_text($this->stmt, $_i + 1, $_text, strlen($_text));
                }
            }
        } elseif ($this->bindTypes !== "") {
            if (strlen($this->bindTypes) != $this->param_count) {
                $this->syntheticFailure(2034, "Number of variables doesn't match number of parameters in prepared statement", "HY000");
                return false;
            }
            $_n = strlen($this->bindTypes);
            for ($_i = 0; $_i < $_n; $_i++) {
                // The values captured by the most recent bind_param call.
                $_value = $this->boundParams[$_i];
                $_char = substr($this->bindTypes, $_i, 1);
                if ($_value === null) {
                    elephc_pdo_bind_null($this->stmt, $_i + 1);
                } elseif ($_char === "i") {
                    elephc_pdo_bind_int($this->stmt, $_i + 1, (int) $_value);
                } elseif ($_char === "d") {
                    elephc_pdo_bind_double($this->stmt, $_i + 1, (float) $_value);
                } elseif ($_char === "b") {
                    $_blob = (string) $_value;
                    elephc_pdo_bind_blob($this->stmt, $_i + 1, $_blob, strlen($_blob));
                } else {
                    $_text = (string) $_value;
                    elephc_pdo_bind_text($this->stmt, $_i + 1, $_text, strlen($_text));
                }
            }
        } elseif ($this->param_count > 0) {
            $this->syntheticFailure(2031, "No data supplied for parameters in prepared statement", "HY000");
            return false;
        }
        $_rc = elephc_pdo_step($this->stmt);
        if ($_rc < 0) {
            return $this->opFailed();
        }
        $_cols = elephc_pdo_column_count($this->stmt);
        $this->field_count = $_cols;
        if ($_cols == 0) {
            $this->affected_rows = elephc_pdo_changes($this->conn);
            $this->insert_id = elephc_pdo_last_insert_id($this->conn, "");
            $this->refreshLink();
            // Rewind now so the statement is immediately re-executable.
            elephc_pdo_reset($this->stmt);
            $this->clearError();
            return true;
        }
        // A result set exists: the first step's verdict (1 = row, 0 = empty)
        // is kept pending; get_result()/store_result() drain from here.
        $this->hasPending = true;
        $this->pendingStep = $_rc;
        $this->num_rows = 0;
        $this->refreshLink();
        $this->clearError();
        return true;
    }

    // php refreshes the OWNING connection's affected_rows / insert_id /
    // warning_count from every command's OK packet, so the canonical
    // `$stmt->execute(); $db->insert_id;` idiom reads the statement's value —
    // not a stale one. Mirror the statement's freshly-updated copies onto the
    // link (a no-op if the statement was detached from its connection).
    private function refreshLink(): void {
        if ($this->link !== null) {
            $this->link->affected_rows = $this->affected_rows;
            $this->link->insert_id = $this->insert_id;
            $this->link->warning_count = elephc_pdo_warning_count($this->conn);
        }
    }

    public function result_metadata(): mysqli_result|false {
        if ($this->stmt < 0) {
            $this->syntheticFailure(2050, "mysqli_stmt object is already closed", "HY000");
            return false;
        }
        $_cols = elephc_pdo_column_count($this->stmt);
        if ($_cols <= 0) {
            return false;
        }
        $_names = [];
        $_tables = [];
        $_natives = [];
        $_flags = [];
        $_lens = [];
        $_decimals = [];
        for ($_i = 0; $_i < $_cols; $_i++) {
            $_names[] = elephc_pdo_column_name($this->stmt, $_i);
            $_tables[] = elephc_pdo_column_table_name($this->stmt, $_i);
            $_natives[] = elephc_pdo_column_native_type($this->stmt, $_i);
            $_flags[] = elephc_pdo_column_flags($this->stmt, $_i);
            $_lens[] = elephc_pdo_column_len($this->stmt, $_i);
            $_decimals[] = elephc_pdo_column_precision($this->stmt, $_i);
        }
        return mysqli_result::__elephcFromDrain([], 0, $_names, $_tables, $_natives, $_flags, $_lens, $_decimals);
    }

    public function get_result(): mysqli_result|false {
        if ($this->stmt < 0) {
            $this->syntheticFailure(2050, "mysqli_stmt object is already closed", "HY000");
            return false;
        }
        if (!$this->hasPending) {
            return false;
        }
        $_cols = $this->field_count;
        $_names = [];
        $_tables = [];
        $_natives = [];
        $_flags = [];
        $_lens = [];
        $_decimals = [];
        for ($_i = 0; $_i < $_cols; $_i++) {
            $_names[] = elephc_pdo_column_name($this->stmt, $_i);
            $_tables[] = elephc_pdo_column_table_name($this->stmt, $_i);
            $_natives[] = elephc_pdo_column_native_type($this->stmt, $_i);
            $_flags[] = elephc_pdo_column_flags($this->stmt, $_i);
            $_lens[] = elephc_pdo_column_len($this->stmt, $_i);
            $_decimals[] = elephc_pdo_column_precision($this->stmt, $_i);
        }
        $_cells = [];
        $_rowCount = 0;
        $_rc = $this->pendingStep;
        while ($_rc == 1) {
            for ($_i = 0; $_i < $_cols; $_i++) {
                $_cells[] = $this->columnValue($_i);
            }
            $_rowCount = $_rowCount + 1;
            $_rc = elephc_pdo_step($this->stmt);
        }
        $this->hasPending = false;
        if ($_rc < 0) {
            $this->opFailed();
            return false;
        }
        // The statement stays alive and rewound, so it can be re-executed.
        elephc_pdo_reset($this->stmt);
        $this->num_rows = $_rowCount;
        $this->affected_rows = $_rowCount;
        $this->clearError();
        return mysqli_result::__elephcFromDrain($_cells, $_rowCount, $_names, $_tables, $_natives, $_flags, $_lens, $_decimals);
    }

    public function store_result(): bool {
        if ($this->stmt < 0) {
            $this->syntheticFailure(2050, "mysqli_stmt object is already closed", "HY000");
            return false;
        }
        if (!$this->hasPending) {
            // Nothing pending (non-select execute, or already consumed):
            // php-src treats this as a successful no-op.
            return true;
        }
        // Without bind_result/fetch (see the module preamble) the buffered
        // rows have no reader; consume and count them so num_rows is valid.
        $_rowCount = 0;
        $_rc = $this->pendingStep;
        while ($_rc == 1) {
            $_rowCount = $_rowCount + 1;
            $_rc = elephc_pdo_step($this->stmt);
        }
        $this->hasPending = false;
        if ($_rc < 0) {
            return $this->opFailed();
        }
        elephc_pdo_reset($this->stmt);
        $this->num_rows = $_rowCount;
        $this->affected_rows = $_rowCount;
        $this->clearError();
        return true;
    }

    public function reset(): bool {
        if ($this->stmt < 0) {
            return false;
        }
        elephc_pdo_reset($this->stmt);
        elephc_pdo_clear_bindings($this->stmt);
        $this->hasPending = false;
        $this->num_rows = 0;
        $this->clearError();
        return true;
    }

    public function close(): bool {
        if ($this->stmt >= 0) {
            elephc_pdo_finalize($this->stmt);
            $this->stmt = -1;
        }
        // Clear the pending-result flag too: a post-close get_result() /
        // store_result() must see a closed statement (and raise the
        // already-closed error), not drive the bridge with handle -1.
        $this->hasPending = false;
        $this->link = null;
        return true;
    }

    public function __destruct() {
        // The bridge ignores an unknown/already-finalized handle, so this is
        // safe even when the owning mysqli connection was closed first.
        if ($this->stmt >= 0) {
            elephc_pdo_finalize($this->stmt);
            $this->stmt = -1;
        }
        $this->hasPending = false;
        $this->link = null;
    }

    // -- internal helpers ($_-prefixed locals; same checker rule as PDO) --

    // Decodes one cell of the statement's current row; same dispatch as the
    // connection's columnValue (int / float / null / length-counted bytes).
    private function columnValue(int $index): mixed {
        $_type = elephc_pdo_column_type($this->stmt, $index);
        if ($_type == 1) {
            return elephc_pdo_column_int($this->stmt, $index);
        }
        if ($_type == 2) {
            return elephc_pdo_column_double($this->stmt, $index);
        }
        if ($_type == 5) {
            return null;
        }
        $_len = elephc_pdo_column_data_len($this->stmt, $index);
        if ($_len > 0) {
            return __elephc_ptr_read_string(elephc_pdo_column_data_ptr($this->stmt, $index), $_len);
        }
        return "";
    }

    // Records a client-side failure that has no live bridge error state and
    // dispatches mysqli_report (same contract as mysqli::syntheticFailure).
    private function syntheticFailure(int $errno, string $message, string $sqlstate): void {
        $this->errno = $errno;
        $this->error = $message;
        $this->sqlstate = $sqlstate;
        $this->error_list = [["errno" => $errno, "sqlstate" => $sqlstate, "error" => $message]];
        $this->report($message, $errno, $sqlstate);
    }

    // Refreshes errno/error/sqlstate from the live statement error state
    // (falling back to the connection for prepare-level failures), then
    // dispatches mysqli_report. Always returns false so callers tail-call it.
    private function opFailed(): bool {
        $this->errno = elephc_pdo_stmt_errcode($this->stmt);
        $this->error = elephc_pdo_stmt_errmsg($this->stmt);
        $this->sqlstate = elephc_pdo_stmt_sqlstate($this->stmt);
        if ($this->errno == 0 && $this->conn >= 0) {
            $this->errno = elephc_pdo_errcode($this->conn);
            $this->error = elephc_pdo_errmsg($this->conn);
            $this->sqlstate = elephc_pdo_sqlstate($this->conn);
        }
        if ($this->sqlstate === "") {
            $this->sqlstate = "HY000";
        }
        $this->error_list = [["errno" => $this->errno, "sqlstate" => $this->sqlstate, "error" => $this->error]];
        $this->report($this->error, $this->errno, $this->sqlstate);
        return false;
    }

    // Clears the per-operation error state after a successful operation.
    private function clearError(): void {
        $this->errno = 0;
        $this->error = "";
        $this->sqlstate = "00000";
        $this->error_list = [];
    }

    // mysqli_report dispatch: STRICT throws mysqli_sql_exception, ERROR alone
    // writes to STDERR, OFF is silent (same contract as mysqli::report).
    private function report(string $message, int $errno, string $sqlstate): void {
        if ((mysqli::$reportMode & 2) != 0) {
            $_e = new mysqli_sql_exception($message, $errno);
            $_e->sqlstate = $sqlstate;
            throw $_e;
        }
        if ((mysqli::$reportMode & 1) != 0) {
            fwrite(STDERR, "mysqli error: " . $message . "\n");
        }
    }
}
"##;
