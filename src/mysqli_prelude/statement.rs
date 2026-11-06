//! Purpose:
//! The `mysqli_stmt` class, as an elephc-PHP source fragment (the parse-parity
//! oracle for `build::statement`): native prepares over `elephc_pdo_prepare(…, 0)`,
//! reference parameter binding, result binding with `fetch()`, `store_result()`
//! buffering with `data_seek()`, long data, attributes, multiple result sets,
//! and `get_result()` draining into an independent `mysqli_result`.
//!
//! Called from:
//! - `crate::mysqli_prelude::fragments::source_for_version` (concatenated into
//!   the injected prelude).
//!
//! Key details:
//! - `bind_param(string $types, mixed &...$vars)` and `bind_result(mixed &...$vars)`
//!   keep PHP's REFERENCE semantics through closures that capture the by-ref
//!   variadic by reference: `execute()` reads the variables' current values and
//!   `fetch()` writes each row back into them, exactly like php.
//! - `execute` steps once to learn whether a result set exists; `fetch` /
//!   `get_result` / `store_result` consume the rest and rewind the statement
//!   (unless a CALL left another result set, probed for `more_results()`).
//! - `$legacyAttributes` is rewritten per version: PHP < 8.4 still accepts
//!   MYSQLI_STMT_ATTR_PREFETCH_ROWS and the FOR_UPDATE / SCROLLABLE cursor types.

/// The `mysqli_stmt` class (fragment without a `<?php` header).
pub(super) const SRC: &str = r##"
// -- mysqli prepared-statement surface --

class mysqli_stmt {
    public int|string $affected_rows = 0;
    public int|string $insert_id = 0;
    public int|string $num_rows = 0;
    public int $param_count = 0;
    public int $field_count = 0;

    // mysqlnd's per-column cell conversion of the current result set
    // (mysqli_result::__elephcCellKind) and the columns' display widths;
    // rebuilt whenever the set's metadata changes.
    private array $cellKinds = [];
    private array $cellLengths = [];
    public int $errno = 0;
    public array $error_list = [];
    public string $error = "";
    public string $sqlstate = "00000";
    public int $id = 0;

    // PHP < 8.4 also accepts MYSQLI_STMT_ATTR_PREFETCH_ROWS and the FOR_UPDATE /
    // SCROLLABLE cursor types; the literal is rewritten per version (fragments.rs).
    private static bool $legacyAttributes = false;

    // Bridge handles: the prepared-statement handle and the owning connection.
    private int $stmt = -1;
    private int $conn = -1;
    private ?mysqli $link = null;
    private bool $closed = false;
    // The prepared SQL, named by MYSQLI_REPORT_INDEX diagnostics.
    private string $queryText = "";

    // bind_param(): the type string and a closure that reads the bound
    // variables' CURRENT values (captured by reference), so each execute()
    // sends whatever the variables hold at that moment — php's semantics.
    private string $bindTypes = "";
    private ?Closure $paramReader = null;
    // bind_result(): a closure that writes one fetched row into the bound
    // variables (captured by reference).
    private ?Closure $resultWriter = null;
    // Whether each parameter (0-based) received a send_long_data() chunk since the
    // last execute(): the chunks are already on the server (COM_STMT_SEND_LONG_DATA).
    private array $longDataSent = [];

    // Cursor state: execute() pre-steps once; fetch()/get_result()/store_result()
    // consume the pending rows. store_result() buffers them here so data_seek()
    // and fetch() can walk them.
    private bool $executedOnce = false;
    // mysqlnd statistics: executions since prepare (ps_prepared_*_executed) and
    // whether this execution's unbuffered fetch was counted (ps_unbuffered_sets).
    private int $executeCount = 0;
    private bool $unbufferedCounted = false;
    private bool $hasPending = false;
    private int $pendingStep = 0;
    private bool $stored = false;
    private array $storedCells = [];
    private int $storedCount = 0;
    private int $storedPos = 0;
    // Whether the statement holds a further result set (CALL procedures).
    private bool $moreResults = false;
    private int $warningCount = 0;

    // mysqli_stmt::attr_set() state.
    private int $attrUpdateMaxLength = 0;
    private int $attrCursorType = 0;
    // MYSQLI_CURSOR_TYPE_READ_ONLY: this execution's rows sit in a server cursor (fetched
    // with COM_STMT_FETCH, the link stays free), and whether the cursor was read to its end.
    private bool $cursorOpen = false;
    private bool $cursorSpent = false;
    private int $attrPrefetchRows = 1;

    // php: `new mysqli_stmt($mysql, $query)` is mysqli_stmt_init() plus an
    // optional prepare.
    public function __construct(mysqli $mysql, ?string $query = null) {
        $_conn = $mysql->__elephcConn();
        if ($_conn < 0) {
            throw new Error("mysqli object is not fully initialized");
        }
        $this->link = $mysql;
        $this->conn = $_conn;
        if ($query !== null) {
            $this->prepare($query);
        }
    }

    // Internal: mysqli::prepare()'s one-step prepare. On failure php copies the
    // statement's error onto the connection before reporting it.
    private function __elephcPrepareOrCopyError(string $query): bool {
        return $this->prepareHandle($query, true);
    }

    public function prepare(string $query): bool {
        if ($this->closed) {
            throw new Error("mysqli_stmt object is already closed");
        }
        if (!$this->requireLinkNotBusy()) {
            return false;
        }
        return $this->prepareHandle($query, false);
    }

    // Prepares $query natively (real `?` placeholders on the server), dropping
    // any earlier prepared handle and every bind. $copyToLink mirrors a failure
    // onto the owning connection (mysqli::prepare's contract).
    private function prepareHandle(string $query, bool $copyToLink): bool {
        if ($this->stmt >= 0) {
            $_previousLink = $this->link;
            if ($_previousLink !== null) {
                $_previousLink->__elephcStatementIdle($this->stmt);
            }
            elephc_pdo_finalize($this->stmt);
            $this->stmt = -1;
        }
        $this->linkBump("com_stmt_prepare");
        // Like mysqlnd's prepared statements, the result set stays on the server
        // (rows streamed on demand) until store_result()/get_result()/fetch().
        elephc_pdo_set_buffered_query($this->conn, 0);
        $_handle = elephc_pdo_prepare($this->conn, $query, 0);
        elephc_pdo_set_buffered_query($this->conn, 1);
        if ($_handle < 0) {
            $_link = $this->link;
            if ($copyToLink && $_link !== null) {
                $_link->__elephcAdoptBridgeError();
            }
            // stmt is still -1, so opFailed reads the connection's error state
            // (its stmt-errno fallback), which is where a prepare error lives.
            $this->opFailed();
            return false;
        }
        $this->stmt = $_handle;
        $this->queryText = $query;
        $this->param_count = elephc_pdo_mysql_param_count($_handle);
        // Native prepares know their columns before execution, except CALL,
        // whose result shape exists only once the procedure runs.
        $_head = strtoupper(ltrim($query));
        $this->field_count = str_starts_with($_head, "CALL") ? 0 : elephc_pdo_column_count($_handle);
        $this->cellKinds = [];
        $this->id = elephc_pdo_mysql_stmt_id($_handle);
        $this->bindTypes = "";
        $this->paramReader = null;
        $this->resultWriter = null;
        $this->clearLongData();
        $this->executedOnce = false;
        $this->hasPending = false;
        $this->syncLink();
        $this->dropStored();
        $this->moreResults = false;
        $this->syncLink();
        $this->affected_rows = 0;
        $this->insert_id = 0;
        $this->num_rows = 0;
        $this->clearError();
        return true;
    }

    public function bind_param(string $types, mixed &...$vars): bool {
        // The closure captures the variadic BY REFERENCE, so each execute()
        // reads the bound variables' current values — php's reference binding.
        return $this->__elephcBindParamReader("mysqli_stmt::bind_param", 1, $types, count($vars), function () use (&$vars): array {
            $_values = [];
            $_n = count($vars);
            for ($_i = 0; $_i < $_n; $_i++) {
                $_values[] = $vars[$_i];
            }
            return $_values;
        });
    }

    // Shared validation + binding behind bind_param and the procedural
    // mysqli_stmt_bind_param (whose by-ref variadic cannot be spread-forwarded).
    private function __elephcBindParamReader(string $context, int $typesArg, string $types, int $given, Closure $reader): bool {
        $this->requireValid();
        $_want = strlen($types);
        if ($_want == 0) {
            throw new ValueError($context . "(): Argument #" . $typesArg . " (\$types) must not be empty");
        }
        if ($_want != $given) {
            throw new ArgumentCountError("The number of elements in the type definition string must match the number of bind variables");
        }
        if ($_want != $this->param_count) {
            throw new ArgumentCountError("The number of variables must match the number of parameters in the prepared statement");
        }
        for ($_i = 0; $_i < $_want; $_i++) {
            $_char = substr($types, $_i, 1);
            if ($_char !== "i" && $_char !== "d" && $_char !== "s" && $_char !== "b") {
                throw new ValueError($context . "(): Argument #" . $typesArg . " (\$types) must only contain the \"b\", \"d\", \"i\", \"s\" type specifiers");
            }
        }
        $this->bindTypes = $types;
        $this->paramReader = $reader;
        // mysqlnd's bind_param / bind_result clear the statement error (SET_EMPTY_ERROR).
        $this->clearError();
        return true;
    }

    public function bind_result(mixed &...$vars): bool {
        // Captured BY REFERENCE: every fetch() writes the row into the caller's
        // variables, exactly like php's result binding.
        return $this->__elephcBindResultWriter(count($vars), function (array $row) use (&$vars): void {
            $_n = count($vars);
            for ($_i = 0; $_i < $_n; $_i++) {
                $vars[$_i] = $row[$_i];
            }
        });
    }

    // Shared validation behind bind_result and the procedural
    // mysqli_stmt_bind_result.
    private function __elephcBindResultWriter(int $given, Closure $writer): bool {
        $this->requireValid();
        if ($given != $this->field_count) {
            throw new ArgumentCountError("Number of bind variables doesn't match number of fields in prepared statement");
        }
        $this->resultWriter = $writer;
        $this->clearError();
        return true;
    }

    public function execute(?array $params = null): bool {
        $this->requireValid();
        if ($params !== null) {
            if (!array_is_list($params)) {
                throw new ValueError("mysqli_stmt::execute(): Argument #1 (\$params) must be a list array");
            }
            $_given = count($params);
            if ($_given != $this->param_count) {
                throw new ValueError("mysqli_stmt::execute(): Argument #1 (\$params) must consist of exactly " . $this->param_count . " elements, " . $_given . " present");
            }
        }
        if (!$this->requireLinkNotBusy()) {
            return false;
        }
        if ($this->hasPending && $this->cursorOpen) {
            $this->drainCursor();
        }
        if ($this->executedOnce) {
            // Re-execution: rewind the server-side cursor and drop old binds.
            elephc_pdo_reset($this->stmt);
            elephc_pdo_clear_bindings($this->stmt);
        }
        $this->executedOnce = true;
        $this->hasPending = false;
        $this->syncLink();
        $this->pendingStep = 0;
        $this->dropStored();
        $this->moreResults = false;
        $this->syncLink();
        if ($params !== null) {
            // PHP 8.1+: execute's own $params bind as strings, in order.
            $_n = count($params);
            for ($_i = 0; $_i < $_n; $_i++) {
                $_value = $params[$_i];
                if ($_value === null) {
                    elephc_pdo_bind_null($this->stmt, $_i + 1);
                } else {
                    $_text = (string) $_value;
                    elephc_pdo_bind_text($this->stmt, $_i + 1, $_text, strlen($_text));
                }
            }
        } elseif ($this->bindTypes !== "") {
            $_values = $this->readBoundParams();
            $_n = strlen($this->bindTypes);
            for ($_i = 0; $_i < $_n; $_i++) {
                // The bound variable's value RIGHT NOW (reference semantics).
                $_value = $_values[$_i];
                $_char = substr($this->bindTypes, $_i, 1);
                if ($_char === "b") {
                    // mysqlnd sends a "b" parameter only through send_long_data() (the
                    // execution leaves a sent one's value out); a NULL variable stays NULL,
                    // anything else without chunks is "".
                    $_sent = (bool) $this->longDataSent[$_i];
                    if ($_value === null && !$_sent) {
                        elephc_pdo_bind_null($this->stmt, $_i + 1);
                    } else {
                        elephc_pdo_bind_blob($this->stmt, $_i + 1, "", 0);
                    }
                } elseif ($_value === null) {
                    elephc_pdo_bind_null($this->stmt, $_i + 1);
                } elseif ($_char === "i") {
                    elephc_pdo_bind_int($this->stmt, $_i + 1, (int) $_value);
                } elseif ($_char === "d") {
                    elephc_pdo_bind_double($this->stmt, $_i + 1, (float) $_value);
                } else {
                    $_text = (string) $_value;
                    elephc_pdo_bind_text($this->stmt, $_i + 1, $_text, strlen($_text));
                }
            }
        } elseif ($this->param_count > 0) {
            $this->syntheticFailure(2031, "No data supplied for parameters in prepared statement", "HY000");
            return false;
        }
        // mysqlnd forgets the long-data chunks once they were sent.
        $this->clearLongData();
        $this->linkBump("com_stmt_execute");
        // The result set's header and metadata only: rows are read when they
        // are stored or fetched, like mysqlnd.
        elephc_pdo_mysql_set_cursor_type($this->stmt, $this->attrCursorType);
        $_rc = elephc_pdo_mysql_execute($this->stmt);
        if ($_rc < 0) {
            return $this->opFailed();
        }
        $this->cursorOpen = elephc_pdo_mysql_cursor_open($this->stmt) == 1;
        $this->cursorSpent = false;
        $_cols = elephc_pdo_column_count($this->stmt);
        $this->field_count = $_cols;
        $this->cellKinds = [];
        $this->executeCount = $this->executeCount + 1;
        $this->unbufferedCounted = false;
        $_statLink = $this->link;
        if ($_statLink !== null) {
            $_statLink->__elephcHeaderStats($_cols, false);
        }
        $this->warningCount = elephc_pdo_warning_count($this->conn);
        if ($_cols == 0) {
            $this->affected_rows = elephc_pdo_changes($this->conn);
            $this->insert_id = mysqli::__elephcUnsignedId(elephc_pdo_last_insert_id_text($this->conn, ""));
            $this->refreshLink();
            // Rewind now so the statement is immediately re-executable.
            elephc_pdo_reset($this->stmt);
            $this->clearError();
            $this->reportIndex();
            return true;
        }
        // A result set exists: the first step's verdict (1 = row, 0 = empty)
        // is kept pending; fetch()/get_result()/store_result() drain from here.
        $this->hasPending = true;
        $this->syncLink();
        $this->pendingStep = 2;
        $this->num_rows = 0;
        $this->refreshLink();
        $this->clearError();
        $this->reportIndex();
        return true;
    }

    // MYSQLI_REPORT_INDEX for this statement's last execution (php names the
    // prepared SQL in the "No index used …" diagnostic).
    private function reportIndex(): void {
        $_link = $this->link;
        if ($_link !== null) {
            $_link->__elephcReportIndex($this->queryText);
        }
    }

    public function fetch(): ?bool {
        $this->requireValid();
        if ($this->stored) {
            // Buffered by store_result(): walk the client-side rows.
            if ($this->storedPos >= $this->storedCount) {
                return null;
            }
            $_row = [];
            $_base = $this->storedPos * $this->field_count;
            for ($_i = 0; $_i < $this->field_count; $_i++) {
                $_row[] = $this->storedCells[$_base + $_i];
            }
            $this->storedPos = $this->storedPos + 1;
            $this->linkStat("rows_fetched_from_client_ps_buffered", 1, false);
            $this->writeRow($_row);
            return true;
        }
        if (!$this->hasPending) {
            if (!$this->executedOnce) {
                $this->syntheticFailure(2014, "Commands out of sync; you can't run this command now", "HY000");
                return false;
            }
            if ($this->cursorSpent) {
                // Past a cursor's end mysqlnd sends one more COM_STMT_FETCH and fails,
                // dropping the server's error (nothing recorded or reported).
                $this->linkBump("com_stmt_fetch");
                elephc_pdo_mysql_stmt_fetch_past_end($this->stmt);
                return false;
            }
            return null;
        }
        if ($this->cursorOpen && $this->cursorLinkBusy()) {
            return false;
        }
        // The first fetch of an unstored execution is mysqlnd's use_result().
        if (!$this->unbufferedCounted) {
            $this->unbufferedCounted = true;
            $this->linkStat("ps_unbuffered_sets", 1, true);
        }
        if ($this->cursorOpen) {
            $this->linkBump("com_stmt_fetch");
        }
        $this->pendingRow();
        if ($this->pendingStep < 0) {
            // The previous advance failed: report it now.
            $this->hasPending = false;
            $this->syncLink();
            $this->opFailed();
            return false;
        }
        if ($this->pendingStep == 0) {
            $this->cursorSpent = $this->cursorOpen;
            $this->finishResultSet();
            return null;
        }
        $_row = [];
        for ($_i = 0; $_i < $this->field_count; $_i++) {
            $_row[] = $this->columnValue($_i);
        }
        $this->num_rows = $this->num_rows + 1;
        $this->linkStat("rows_fetched_from_server_ps", 1, true);
        $this->linkStat($this->cursorOpen ? "rows_fetched_from_client_ps_cursor" : "rows_fetched_from_client_ps_unbuffered", 1, true);
        // The next row (or the set's EOF) is read by the next fetch(), like mysqlnd.
        $this->pendingStep = 2;
        $this->writeRow($_row);
        return true;
    }

    public function get_result(): mysqli_result|false {
        $this->requireValid();
        if ($this->stored) {
            // mysqlnd refuses get_result() once store_result() buffered the set: the
            // two APIs cannot be combined, and the stored rows stay fetchable.
            $this->syntheticFailure(2014, "Commands out of sync; you can't run this command now", "HY000");
            return false;
        }
        if (!$this->hasPending) {
            if (!$this->executedOnce) {
                $this->syntheticFailure(2014, "Commands out of sync; you can't run this command now", "HY000");
            }
            return false;
        }
        if ($this->cursorOpen) {
            if ($this->cursorLinkBusy()) {
                return false;
            }
            // mysqlnd reads every remaining cursor row with one COM_STMT_FETCH.
            $this->linkBump("com_stmt_fetch");
            if (elephc_pdo_mysql_cursor_fetch_all($this->stmt) < 0) {
                $this->hasPending = false;
                $this->syncLink();
                $this->opFailed();
                return false;
            }
        }
        $_cells = [];
        $_rowCount = 0;
        $_rc = $this->pendingRow();
        while ($_rc == 1) {
            for ($_i = 0; $_i < $this->field_count; $_i++) {
                $_cells[] = $this->columnValue($_i);
            }
            $_rowCount = $_rowCount + 1;
            $_rc = elephc_pdo_step($this->stmt);
        }
        $this->hasPending = false;
        $this->syncLink();
        if ($_rc < 0) {
            $this->opFailed();
            return false;
        }
        // mysqlnd's get_result() buffers through the normal-result path.
        $this->linkBump("buffered_sets");
        $this->linkStat("rows_fetched_from_server_ps", $_rowCount, true);
        $this->linkStat("rows_buffered_from_client_ps", $_rowCount, true);
        $this->num_rows = $_rowCount;
        $this->affected_rows = $_rowCount;
        $this->finishResultSet();
        $this->clearError();
        return $this->buildResult($_cells, $_rowCount);
    }

    public function store_result(): bool {
        $this->requireValid();
        if (!$this->hasPending) {
            // Nothing pending (non-select execute, or already consumed):
            // php-src treats this as a successful no-op.
            return true;
        }
        // Buffer the remaining rows client-side so fetch() / data_seek() walk
        // them and num_rows is exact.
        if ($this->cursorOpen) {
            if ($this->cursorLinkBusy()) {
                return false;
            }
            // mysqlnd reads every remaining cursor row with one COM_STMT_FETCH.
            $this->linkBump("com_stmt_fetch");
            if (elephc_pdo_mysql_cursor_fetch_all($this->stmt) < 0) {
                $this->hasPending = false;
                $this->syncLink();
                return $this->opFailed();
            }
        }
        $_cells = [];
        $_rowCount = 0;
        $_rc = $this->pendingRow();
        while ($_rc == 1) {
            for ($_i = 0; $_i < $this->field_count; $_i++) {
                $_cells[] = $this->columnValue($_i);
            }
            $_rowCount = $_rowCount + 1;
            $_rc = elephc_pdo_step($this->stmt);
        }
        $this->hasPending = false;
        $this->syncLink();
        if ($_rc < 0) {
            return $this->opFailed();
        }
        $this->linkBump("ps_buffered_sets");
        $this->linkStat("rows_fetched_from_server_ps", $_rowCount, true);
        $this->linkStat("rows_buffered_from_client_ps", $_rowCount, true);
        $this->stored = true;
        $this->storedCells = $_cells;
        $this->storedCount = $_rowCount;
        $this->storedPos = 0;
        $this->num_rows = $_rowCount;
        $this->affected_rows = $_rowCount;
        $this->finishResultSet();
        $this->clearError();
        return true;
    }

    public function data_seek(int $offset): void {
        if ($offset < 0) {
            throw new ValueError("mysqli_stmt::data_seek(): Argument #1 (\$offset) must be greater than or equal to 0");
        }
        $this->requireValid();
        // Meaningful on a store_result()-buffered statement (php's contract);
        // an offset past the end leaves the cursor exhausted.
        if ($this->stored) {
            $this->storedPos = $offset > $this->storedCount ? $this->storedCount : $offset;
        }
    }

    public function num_rows(): int|string {
        $this->requireValid();
        return $this->num_rows;
    }

    public function result_metadata(): mysqli_result|false {
        $this->requireValid();
        if ($this->field_count == 0) {
            return false;
        }
        // No rows: an empty slice of the cell buffer keeps the cell list's
        // element type (a bare `[]` literal would narrow the parameter).
        return $this->buildResult(array_slice($this->storedCells, 0, 0), 0);
    }

    public function more_results(): bool {
        $this->requireValid();
        return $this->moreResults;
    }

    public function next_result(): bool {
        $this->requireValid();
        if (!$this->moreResults) {
            return false;
        }
        $this->moreResults = false;
        $this->syncLink();
        // Read the next set's header and metadata; its rows stay unread.
        $_advanced = elephc_pdo_next_rowset($this->stmt);
        if ($_advanced < 0) {
            return $this->opFailed();
        }
        if ($_advanced == 0) {
            elephc_pdo_reset($this->stmt);
            return false;
        }
        $_rc = 2;
        $this->dropStored();
        $this->field_count = elephc_pdo_column_count($this->stmt);
        $this->cellKinds = [];
        $this->num_rows = 0;
        if ($this->field_count == 0) {
            $this->hasPending = false;
            $this->syncLink();
            $this->affected_rows = elephc_pdo_changes($this->conn);
            $this->finishResultSet();
        } else {
            $this->hasPending = true;
            $this->syncLink();
            $this->pendingStep = $_rc;
        }
        $this->clearError();
        return true;
    }

    public function send_long_data(int $param_num, string $data): bool {
        if ($param_num < 0) {
            throw new ValueError("mysqli_stmt::send_long_data(): Argument #1 (\$param_num) must be greater than or equal to 0");
        }
        $this->requireValid();
        // mysqlnd's order: no bound parameters is "out of sync", then the parameter
        // number, then a parameter not bound as `b` (MYSQL_TYPE_LONG_BLOB). None of
        // them reaches mysqli_report(), like php.
        if ($this->bindTypes === "") {
            $this->unreportedFailure(2014, "Commands out of sync; you can't run this command now");
            return false;
        }
        if ($param_num >= $this->param_count) {
            $this->unreportedFailure(2034, "Invalid parameter number");
            return false;
        }
        if (substr($this->bindTypes, $param_num, 1) !== "b") {
            $this->unreportedFailure(2035, "Can't send long data for non-string/non-binary data types");
            return false;
        }
        $this->clearError();
        // mysqlnd sends a chunk only on a ready link: while another result or an
        // async query owns it, the call fails without an error and the chunk is
        // dropped.
        $_link = $this->link;
        if ($_link !== null && $_link->__elephcBusyFor($this->stmt)) {
            return false;
        }
        // Each chunk goes to the server at once (COM_STMT_SEND_LONG_DATA, no response);
        // a lost connection fails without mysqli_report(), like mysqlnd.
        $_rc = elephc_pdo_mysql_send_long_data($this->stmt, $param_num, $data, strlen($data));
        if ($_rc == 0) {
            return false;
        }
        $this->linkBump("com_stmt_send_long_data");
        if ($_rc < 0) {
            $this->adoptBridgeError();
            if ($_link !== null) {
                $_link->__elephcAdoptBridgeError();
            }
            return false;
        }
        $this->longDataSent[$param_num] = true;
        return true;
    }

    public function attr_get(int $attribute): int {
        $this->requireValid();
        if ($attribute == 0) {
            return $this->attrUpdateMaxLength;
        }
        if ($attribute == 1) {
            return $this->attrCursorType;
        }
        if ($attribute == 2 && mysqli_stmt::$legacyAttributes) {
            return $this->attrPrefetchRows;
        }
        throw new ValueError($this->attributeError("mysqli_stmt::attr_get"));
    }

    public function attr_set(int $attribute, int $value): bool {
        $this->requireValid();
        if ($attribute == 0) {
            if ($value != 0 && $value != 1) {
                throw new ValueError("mysqli_stmt::attr_set(): Argument #2 (\$value) must be 0 or 1 for attribute MYSQLI_STMT_ATTR_UPDATE_MAX_LENGTH");
            }
            $this->attrUpdateMaxLength = $value;
            return true;
        }
        if ($attribute == 1) {
            $_legal = $value == 0 || $value == 1
                || (mysqli_stmt::$legacyAttributes && ($value == 2 || $value == 4));
            if (!$_legal) {
                throw new ValueError("mysqli_stmt::attr_set(): Argument #2 (\$value) must be one of the MYSQLI_CURSOR_TYPE_* constants for attribute MYSQLI_STMT_ATTR_CURSOR_TYPE");
            }
            if ($value > 1) {
                // mysqlnd implements only NO_CURSOR and READ_ONLY.
                $this->syntheticFailure(2054, "Not implemented", "HY000");
                return false;
            }
            $this->attrCursorType = $value;
            return true;
        }
        if ($attribute == 2 && mysqli_stmt::$legacyAttributes) {
            if ($value < 1) {
                throw new ValueError("mysqli_stmt::attr_set(): Argument #2 (\$value) must be greater than 0 for attribute MYSQLI_STMT_ATTR_PREFETCH_ROWS");
            }
            if ($value > 1) {
                $this->syntheticFailure(2054, "Not implemented", "HY000");
                return false;
            }
            $this->attrPrefetchRows = $value;
            return true;
        }
        throw new ValueError($this->attributeError("mysqli_stmt::attr_set"));
    }

    public function get_warnings(): mysqli_warning|false {
        $this->requireValid();
        $_link = $this->link;
        if ($_link === null || $this->warningCount == 0) {
            return false;
        }
        return $_link->__elephcFetchWarnings();
    }

    public function free_result(): void {
        $this->requireValid();
        // Discards pending or buffered rows so the statement can be re-executed.
        if ($this->hasPending && $this->cursorOpen) {
            $this->drainCursor();
        }
        if ($this->hasPending && $this->stmt >= 0) {
            elephc_pdo_reset($this->stmt);
        }
        $this->hasPending = false;
        $this->syncLink();
        $this->moreResults = false;
        $this->syncLink();
        $this->dropStored();
        // mysqlnd reads num_rows from the result set, which is gone now.
        $this->num_rows = 0;
    }

    public function reset(): bool {
        $this->requireValid();
        if ($this->hasPending && $this->cursorOpen) {
            $this->drainCursor();
        }
        // mysqlnd sends COM_STMT_RESET only on a ready link: while another result or
        // an async query owns it, the reset is local and succeeds without traffic.
        $_link = $this->link;
        $_rc = 1;
        if ($_link !== null && $_link->__elephcBusyFor($this->stmt)) {
            elephc_pdo_reset($this->stmt);
        } else {
            $this->linkBump("com_stmt_reset");
            $_rc = elephc_pdo_mysql_stmt_reset($this->stmt);
        }
        // php's reset keeps the parameter and result bindings but discards
        // long data and any pending (unread) rows. A set store_result() already
        // buffered survives with its position and num_rows, like mysqlnd's;
        // re-execution, free_result() or close() drop it.
        $this->clearLongData();
        $this->hasPending = false;
        $this->syncLink();
        $this->moreResults = false;
        $this->syncLink();
        if (!$this->stored) {
            $this->num_rows = 0;
        }
        if ($_rc < 0) {
            // A failed reset (a lost connection) fails without mysqli_report(), like
            // mysqli_stmt_reset(); the error is the statement's and the link's.
            $this->adoptBridgeError();
            if ($_link !== null) {
                $_link->__elephcAdoptBridgeError();
            }
            return false;
        }
        $this->clearError();
        return true;
    }

    public function close(): bool {
        $this->requireValid();
        $this->countClose(true);
        $this->linkBump("com_stmt_close");
        $this->release();
        $this->closed = true;
        return true;
    }

    public function __destruct() {
        // The bridge ignores an unknown/already-finalized handle, so this is
        // safe even when the owning mysqli connection was closed first.
        if (!$this->closed && $this->stmt >= 0) {
            $this->countClose(false);
        }
        $this->release();
    }

    // mysqlnd's statement close accounting (mysqlnd_stmt::net_close / dtor): a
    // result set still on the wire is flushed row by row, the statement's
    // result metadata is freed (implicitly), and the close and its execution
    // count are recorded — several of them process-wide only.
    private function countClose(bool $explicit): void {
        if ($this->link === null) {
            return;
        }
        if ($this->hasPending && !$this->stored && $this->cursorOpen) {
            $this->drainCursor();
        } elseif ($this->hasPending && !$this->stored) {
            $this->linkStat("flushed_ps_sets", 1, true);
            $_rc = $this->pendingRow();
            while ($_rc == 1) {
                $this->linkStat("rows_fetched_from_server_ps", 1, true);
                $this->linkStat("rows_fetched_from_client_ps_unbuffered", 1, true);
                $this->linkStat("rows_skipped_ps", 1, true);
                $_rc = elephc_pdo_step($this->stmt);
            }
            $this->pendingStep = $_rc;
        }
        if ($this->field_count > 0) {
            $this->linkStat("implicit_free_result", 1, true);
        }
        $this->linkStat($explicit ? "explicit_free_result" : "implicit_free_result", 1, false);
        if ($this->executeCount == 0) {
            $this->linkStat("ps_prepared_never_executed", 1, true);
        } elseif ($this->executeCount == 1) {
            $this->linkStat("ps_prepared_once_executed", 1, true);
        }
        $this->linkStat($explicit ? "explicit_stmt_close" : "implicit_stmt_close", 1, false);
    }

    // -- internal helpers ($_-prefixed locals; same checker rule as PDO) --

    // The verdict of the result set's first row (1 = row, 0 = empty, < 0 =
    // error), read from the server on first use: execute() leaves the rows
    // unread (pendingStep 2).
    private function pendingRow(): int {
        if ($this->pendingStep == 2) {
            $this->pendingStep = elephc_pdo_step($this->stmt);
        }
        return $this->pendingStep;
    }

    // Reads and discards an open cursor's remaining rows, one COM_STMT_FETCH each
    // plus the one that finds its end (mysqlnd's flush on free / reset / close /
    // re-execute).
    private function drainCursor(): void {
        $this->linkStat("flushed_ps_sets", 1, true);
        $this->linkBump("com_stmt_fetch");
        $_rc = $this->pendingRow();
        while ($_rc == 1) {
            $this->linkStat("rows_fetched_from_server_ps", 1, true);
            $this->linkStat("rows_fetched_from_client_ps_cursor", 1, true);
            $this->linkStat("rows_skipped_ps", 1, true);
            $this->linkBump("com_stmt_fetch");
            $_rc = elephc_pdo_step($this->stmt);
        }
        $this->pendingStep = $_rc;
    }

    // Registers (or releases) this statement's unread result set on the link,
    // which then answers other commands with 2014 like mysqlnd.
    private function syncLink(): void {
        $_link = $this->link;
        if ($_link === null || $this->stmt < 0) {
            return;
        }
        // An open cursor's rows stay on the server without blocking the link.
        if (($this->hasPending && !$this->cursorOpen) || $this->moreResults) {
            $_link->__elephcStatementBusy($this->stmt);
        } else {
            $_link->__elephcStatementIdle($this->stmt);
        }
    }

    // Finalizes the bridge statement and drops every per-execution state.
    private function release(): void {
        $_link = $this->link;
        if ($_link !== null && $this->stmt >= 0) {
            $_link->__elephcStatementIdle($this->stmt);
        }
        if ($this->stmt >= 0) {
            elephc_pdo_finalize($this->stmt);
            $this->stmt = -1;
        }
        $this->hasPending = false;
        $this->syncLink();
        $this->moreResults = false;
        $this->syncLink();
        $this->dropStored();
        $this->paramReader = null;
        $this->resultWriter = null;
        $this->link = null;
    }

    // php 8: methods on an unprepared statement raise "not fully initialized",
    // on a closed one "already closed" (Error, regardless of mysqli_report).
    private function requireValid(): void {
        if ($this->closed) {
            throw new Error("mysqli_stmt object is already closed");
        }
        if ($this->stmt < 0) {
            throw new Error("mysqli_stmt object is not fully initialized");
        }
    }

    // The ValueError text for an unknown statement attribute (php 8.4 dropped
    // MYSQLI_STMT_ATTR_PREFETCH_ROWS from the accepted set and the message).
    private function attributeError(string $context): string {
        if (mysqli_stmt::$legacyAttributes) {
            return $context . "(): Argument #1 (\$attribute) must be one of MYSQLI_STMT_ATTR_UPDATE_MAX_LENGTH, MYSQLI_STMT_ATTR_PREFETCH_ROWS, or STMT_ATTR_CURSOR_TYPE";
        }
        return $context . "(): Argument #1 (\$attribute) must be either MYSQLI_STMT_ATTR_UPDATE_MAX_LENGTH or MYSQLI_STMT_ATTR_CURSOR_TYPE";
    }

    // The bind_param() variables' CURRENT values, read through the reference
    // closure (empty when nothing was bound).
    private function readBoundParams(): array {
        $_reader = $this->paramReader;
        if ($_reader === null) {
            return [];
        }
        return (array) $_reader();
    }

    // Writes one fetched row into the bind_result() variables (if any).
    private function writeRow(array $row): void {
        $_writer = $this->resultWriter;
        $_row = $row;
        if ($_writer !== null) {
            $_writer($_row);
        }
    }

    // Resets every parameter's long-data slot to "no chunk sent".
    private function clearLongData(): void {
        $_sent = [];
        for ($_i = 0; $_i < $this->param_count; $_i++) {
            $_sent[] = false;
        }
        $this->longDataSent = $_sent;
    }

    // Forgets the store_result() buffer.
    private function dropStored(): void {
        $this->stored = false;
        $this->storedCells = [];
        $this->storedCount = 0;
        $this->storedPos = 0;
    }

    // Called once the current result set is fully consumed: probes for a
    // further result set (CALL procedures return several) and, when there is
    // none, rewinds the statement so it is immediately re-executable.
    private function finishResultSet(): void {
        $this->hasPending = false;
        $this->syncLink();
        // The set's closing EOF says whether another follows; it is read only
        // by next_result(), like mysqlnd.
        $this->moreResults = elephc_pdo_mysql_more_results($this->stmt) == 1;
        $this->syncLink();
        if (!$this->moreResults) {
            elephc_pdo_reset($this->stmt);
        }
    }

    // Wraps buffered cells (flat, row-major) in an independent mysqli_result
    // carrying this statement's column metadata.
    private function buildResult(array $cells, int $rowCount): mysqli_result {
        $_names = [];
        $_tables = [];
        $_natives = [];
        $_flags = [];
        $_lens = [];
        $_decimals = [];
        for ($_i = 0; $_i < $this->field_count; $_i++) {
            $_names[] = elephc_pdo_column_name($this->stmt, $_i);
            $_tables[] = elephc_pdo_column_table_name($this->stmt, $_i);
            $_natives[] = elephc_pdo_column_native_type($this->stmt, $_i);
            $_flags[] = elephc_pdo_column_flags($this->stmt, $_i);
            $_lens[] = elephc_pdo_column_len($this->stmt, $_i);
            $_decimals[] = elephc_pdo_column_precision($this->stmt, $_i);
        }
        $_result = mysqli_result::__elephcFromDrain($cells, $rowCount, $_names, $_tables, $_natives, $_flags, $_lens, $_decimals);
        $_statLink = $this->link;
        if ($_statLink !== null) {
            $_result->__elephcCountFor($_statLink, true);
        }
        return $_result;
    }

    // Guards the two-step prepare() and execute() while the LINK has
    // unconsumed results (a multi_query batch or a real_query result):
    // php-src raises CR_COMMANDS_OUT_OF_SYNC (2014) on the statement there.
    private function requireLinkNotBusy(): bool {
        $_link = $this->link;
        if ($_link !== null && $_link->__elephcBusyFor($this->stmt)) {
            $this->syntheticFailure(2014, "Commands out of sync; you can't run this command now", "HY000");
            return false;
        }
        return true;
    }

    // An open cursor reads its rows with COM_STMT_FETCH, which mysqlnd refuses
    // while another result (an unbuffered query, an async query, another
    // statement's unread set) owns the link: 2014 on the statement and the link,
    // and the cursor stays readable once that result is consumed.
    private function cursorLinkBusy(): bool {
        $_link = $this->link;
        if ($_link !== null && $_link->__elephcBusyFor($this->stmt)) {
            $_link->__elephcOutOfSync();
            $this->syntheticFailure(2014, "Commands out of sync; you can't run this command now", "HY000");
            return true;
        }
        return false;
    }

    // Bumps a statistic on the owning connection (no-op once detached).
    private function linkStat(string $key, int $by, bool $connection): void {
        $_link = $this->link;
        if ($_link !== null) {
            $_link->__elephcStat($key, $by, $connection);
        }
    }

    private function linkBump(string $key): void {
        $_link = $this->link;
        if ($_link !== null) {
            $_link->__elephcBump($key);
        }
    }

    // php refreshes the OWNING connection's affected_rows / insert_id /
    // warning_count from every command's OK packet, so the canonical
    // `$stmt->execute(); $db->insert_id;` idiom reads the statement's value.
    private function refreshLink(): void {
        $_link = $this->link;
        if ($_link !== null) {
            $_link->affected_rows = $this->affected_rows;
            $_link->insert_id = $this->insert_id;
            $_link->warning_count = $this->warningCount;
        }
    }

    // Decodes one cell of the statement's current row; same dispatch as the
    // connection's columnValue (int / float / null / length-counted bytes).
    private function columnValue(int $index): mixed {
        if (count($this->cellKinds) != $this->field_count) {
            $this->cellKinds = [];
            $this->cellLengths = [];
            for ($_i = 0; $_i < $this->field_count; $_i++) {
                $this->cellKinds[] = mysqli_result::__elephcCellKind(elephc_pdo_column_native_type($this->stmt, $_i), elephc_pdo_column_flags($this->stmt, $_i), false, true);
                $this->cellLengths[] = elephc_pdo_column_len($this->stmt, $_i);
            }
        }
        return mysqli_result::__elephcConvertCell($this->rawColumnValue($index), (int) $this->cellKinds[$index], (int) $this->cellLengths[$index]);
    }

    private function rawColumnValue(int $index): mixed {
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
        $this->adoptBridgeError();
        $this->report($this->error, $this->errno, $this->sqlstate);
        return false;
    }

    // opFailed() without the mysqli_report() dispatch.
    private function adoptBridgeError(): void {
        $this->error = mysqli::__elephcServerMessage(elephc_pdo_stmt_errmsg($this->stmt));
        $this->errno = mysqli::__elephcClientErrno(elephc_pdo_stmt_errcode($this->stmt), $this->error);
        $this->sqlstate = elephc_pdo_stmt_sqlstate($this->stmt);
        // Without a statement handle (a failed prepare) the bridge's per-statement
        // accessors answer -1 / "": the error lives on the connection.
        if (($this->errno == 0 || $this->stmt < 0) && $this->conn >= 0) {
            $this->error = mysqli::__elephcServerMessage(elephc_pdo_errmsg($this->conn));
            $this->errno = mysqli::__elephcClientErrno(elephc_pdo_errcode($this->conn), $this->error);
            $this->sqlstate = elephc_pdo_sqlstate($this->conn);
        }
        if ($this->sqlstate === "") {
            $this->sqlstate = "HY000";
        }
        $this->error_list = [["errno" => $this->errno, "sqlstate" => $this->sqlstate, "error" => $this->error]];
    }

    // Clears the per-operation error state after a successful operation.
    // A client error recorded on the statement without mysqli_report() dispatch.
    private function unreportedFailure(int $errno, string $message): void {
        $this->errno = $errno;
        $this->error = $message;
        $this->sqlstate = "HY000";
        $this->error_list = [["errno" => $errno, "sqlstate" => "HY000", "error" => $message]];
    }

    private function clearError(): void {
        $this->errno = 0;
        $this->error = "";
        $this->sqlstate = "00000";
        $this->error_list = [];
    }

    // mysqli_report dispatch: STRICT throws mysqli_sql_exception, ERROR alone
    // raises php's E_WARNING, OFF is silent (same contract as mysqli::report).
    private function report(string $message, int $errno, string $sqlstate): void {
        if ((mysqli::$reportMode & 2) != 0) {
            throw mysqli_sql_exception::__elephcCreate($message, $errno, $sqlstate);
        }
        if ((mysqli::$reportMode & 1) != 0) {
            mysqli::__elephcWarn("(" . $sqlstate . "/" . $errno . "): " . $message);
        }
    }
}
"##;
