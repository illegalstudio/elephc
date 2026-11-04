//! Purpose:
//! The `mysqli` connection class, as an elephc-PHP source fragment: DSN
//! construction over `elephc_pdo_open_persistent`, connect-time and per-op
//! error bookkeeping (`connect_errno`/`errno`/`error_list`), `mysqli_report`
//! dispatch, escaping, charset, `select_db`, ping, and transactions.
//!
//! Called from:
//! - `crate::mysqli_prelude::fragments::source_for_version` (concatenated into
//!   the injected prelude).
//!
//! Key details:
//! - `$conn = -1` means "not connected" (`mysqli_init()` / argument-less
//!   `new mysqli()`); a successful `real_connect` stores the bridge handle. The
//!   DSN prefix is always forced to `mysql:` and a successful open whose
//!   `elephc_pdo_driver_name` is not "mysql" is rejected (belt-and-braces).
//! - Failure dispatch follows `mysqli::$reportMode` (see `exception.rs`):
//!   STRICT throws `mysqli_sql_exception`, ERROR writes to STDERR and the
//!   caller returns `false`, OFF is silent — mirroring PHP 8.1+'s default.
//!   `PDOException` is never thrown.
//! - `real_escape_string` reuses the MySQL branch of `PDO::quote()` minus the
//!   wrapping quotes and the `_binary` introducer, including the
//!   `NO_BACKSLASH_ESCAPES` quote-doubling fallback (mysqlnd's own behavior).
//! - Method-local variables use the `$_` prefix (same checker clash rule as the
//!   PDO prelude). Public properties are refreshed after operations; writes to
//!   them stick (documented divergence, no write barriers in v1).

/// The `mysqli` class and its connection-level behavior (fragment without a
/// `<?php` header).
pub(super) const SRC: &str = r##"
// -- mysqli connection surface --

class mysqli {
    // Opaque elephc_pdo bridge connection handle; -1 = not connected. Not part
    // of PHP's surface: private, handed to mysqli_stmt through its factory.
    private int $conn = -1;

    // Process-wide mysqli_report() mode. The literal default is rewritten at
    // injection time for --php-version 8.0 (OFF); see fragments.rs.
    public static int $reportMode = 3;

    // Process-wide last-connect failure, read by the no-argument procedural
    // mysqli_connect_errno() / mysqli_connect_error() exactly like PHP's
    // globals; updated by every construct / real_connect attempt.
    public static int $lastConnectErrno = 0;
    public static string $lastConnectError = "";

    // Public properties refreshed after operations (writes stick; documented
    // divergence — no write barriers in v1).
    public int $affected_rows = 0;
    public int $connect_errno = 0;
    public ?string $connect_error = null;
    public int $errno = 0;
    public string $error = "";
    public array $error_list = [];
    public int $field_count = 0;
    public string $client_info = "";
    public int $client_version = 0;
    public string $host_info = "";
    public int $protocol_version = 10;
    public string $server_info = "";
    public int $server_version = 0;
    public string $info = "";
    public int $insert_id = 0;
    public string $sqlstate = "00000";
    public int $thread_id = 0;
    public int $warning_count = 0;

    // mysqli_options() values collected before real_connect applies them.
    private int $optConnectTimeout = 0;
    private string $optInitCommand = "";
    private string $optCharsetName = "";
    // Buffered result produced by real_query() and picked up by store_result()
    // (and, in the multi_query path, by next_result()).
    private ?mysqli_result $pendingResult = null;
    // multi_query() batch state: the live bridge statement (kept alive until
    // every result set is consumed) and the eager next_rowset probe verdict.
    private int $multiStmt = -1;
    private bool $multiMore = false;

    public function __construct(
        ?string $hostname = null,
        ?string $username = null,
        ?string $password = null,
        ?string $database = null,
        ?int $port = null,
        ?string $socket = null
    ) {
        // The argument-less constructor is mysqli_init(): no connection attempt
        // until real_connect() (php-src behavior).
        if ($hostname === null && $username === null && $password === null
            && $database === null && $port === null && $socket === null) {
            return;
        }
        $this->real_connect($hostname, $username, $password, $database, $port, $socket, 0);
    }

    public function real_connect(
        ?string $hostname = null,
        ?string $username = null,
        ?string $password = null,
        ?string $database = null,
        ?int $port = null,
        ?string $socket = null,
        int $flags = 0
    ): bool {
        if (($flags & 2048) != 0) {
            // MYSQLI_CLIENT_SSL is declared but unsupported: fail loudly rather
            // than silently connecting in cleartext.
            return $this->connectFailure(2054, "elephc mysqli does not support MYSQLI_CLIENT_SSL; use PDO MySQL TLS attributes", "HY000");
        }
        if ($this->conn >= 0) {
            // php-src reconnect semantics: mysqlnd closes the existing
            // connection before dialing the new one, so a second real_connect
            // never strands the previous bridge handle (or leaves a persistent
            // handle checked out of the pool).
            $this->multiClose();
            if (elephc_pdo_in_transaction($this->conn) === 1) {
                elephc_pdo_rollback($this->conn);
            }
            elephc_pdo_release($this->conn, 0);
            $this->conn = -1;
            $this->pendingResult = null;
        }
        $_host = $hostname === null ? "localhost" : $hostname;
        $_persistent = 0;
        // php-src compares the `p:` persistent-connection prefix
        // case-insensitively, so `P:host` is persistent too; the real host is
        // the remainder.
        if (strlen($_host) >= 2 && strtolower(substr($_host, 0, 2)) === "p:") {
            $_persistent = 1;
            $_host = substr($_host, 2);
        }
        $_port = $port === null ? 3306 : $port;
        $_socket = $socket === null ? "" : $socket;
        // SECURITY: host, dbname, and unix_socket are folded verbatim into the
        // bridge DSN, which splits on ';' and applies the LAST duplicate of a
        // directive; unlike user/password these three are NOT percent-decoded
        // by the bridge, so a ';' in any of them would inject a second
        // directive (e.g. host="localhost;host=attacker" redirects the
        // connection). None of the three ever legitimately contains a ';', so
        // reject it rather than silently trusting the crafted value.
        if (strpos($_host, ";") !== false || ($database !== null && strpos($database, ";") !== false) || strpos($_socket, ";") !== false) {
            return $this->connectFailure(2002, "elephc mysqli: host, database, and socket must not contain ';'", "HY000");
        }
        // php-src mysqli honors the socket only when the host is empty or
        // exactly "localhost"; any other host goes over TCP.
        $_dsn = "mysql:";
        if ($_socket !== "" && ($_host === "" || $_host === "localhost")) {
            $_dsn = $_dsn . "unix_socket=" . $_socket;
        } else {
            $_dsn = $_dsn . "host=" . ($_host === "" ? "localhost" : $_host) . ";port=" . $_port;
        }
        if ($database !== null && $database !== "") {
            $_dsn = $_dsn . ";dbname=" . $database;
        }
        if ($username !== null) {
            // '%' first, so the '%' introduced by encoding ';' is not itself
            // re-encoded; the bridge percent-decodes user=/password= DSN values
            // (F-CORE-02), so a ';' or '%' inside a credential survives the
            // DSN's split on ';'. An explicitly-passed empty credential is
            // still transmitted (no !== "" gate).
            $_dsn = $_dsn . ";user=" . str_replace(";", "%3B", str_replace("%", "%25", $username));
        }
        if ($password !== null) {
            $_dsn = $_dsn . ";password=" . str_replace(";", "%3B", str_replace("%", "%25", $password));
        }
        if ($this->optConnectTimeout > 0) {
            $_dsn = $_dsn . ";connect_timeout=" . $this->optConnectTimeout;
        }
        // Client flags map onto the bridge's packed driver config (same format
        // PDO packs for $my_driver_config); FOUND_ROWS rides its own argument.
        $_driverConfig = "";
        if (($flags & 32) != 0) {
            $_driverConfig = $_driverConfig . "compress=1;";
        }
        if (($flags & 256) != 0) {
            $_driverConfig = $_driverConfig . "ignore=1;";
        }
        $_foundRows = (($flags & 2) != 0) ? 1 : 0;
        $_conn = elephc_pdo_open_persistent($_dsn, $_persistent, 0, $this->optInitCommand, "", $_foundRows, "", $_driverConfig);
        if ($_conn < 0) {
            $_message = elephc_pdo_last_open_error();
            $_state = elephc_pdo_last_open_sqlstate();
            if ($_state === "") {
                // Connect-time network failure default, same class the PDO
                // mysql driver falls back to.
                $_state = "HY000";
            }
            $_code = elephc_pdo_last_open_native_code();
            if ($_code == 0) {
                // CR_CONN_HOST_ERROR: generic client-side connect failure.
                $_code = 2002;
            }
            return $this->connectFailure($_code, $_message, $_state);
        }
        // Cannot happen while the DSN prefix is forced to mysql:, but keep the
        // guard: a non-mysql handle must never become a mysqli connection.
        if (elephc_pdo_driver_name($_conn) !== "mysql") {
            elephc_pdo_release($_conn, 0);
            return $this->connectFailure(2002, "elephc mysqli opened a non-mysql bridge connection", "HY000");
        }
        $this->conn = $_conn;
        $this->connect_errno = 0;
        $this->connect_error = null;
        mysqli::$lastConnectErrno = 0;
        mysqli::$lastConnectError = "";
        $this->clearError();
        // Connection information, refreshed once per connect.
        $this->host_info = elephc_pdo_connection_status($_conn);
        $this->server_info = elephc_pdo_server_version($_conn);
        $this->server_version = $this->versionStringToInt($this->server_info);
        $this->client_info = elephc_pdo_client_version($_conn);
        $this->client_version = $this->versionStringToInt($this->client_info);
        // Both come from the handshake with ZERO round-trips, like php-src (which
        // reads them off the handshake packet): the server thread id straight
        // from the bridge, and the negotiated client charset is utf8mb4 (the
        // charset the mysql client sends in its handshake response for any server
        // >= 5.5.3). character_set_name() then answers from this client-side
        // state without ever issuing a statement (so it stays usable mid-batch).
        $this->thread_id = elephc_pdo_mysql_thread_id($_conn);
        $this->warning_count = elephc_pdo_warning_count($_conn);
        if ($this->optCharsetName !== "") {
            // MYSQLI_SET_CHARSET_NAME collected before connect applies now.
            $this->set_charset($this->optCharsetName);
        }
        return true;
    }

    public function connect(
        ?string $hostname = null,
        ?string $username = null,
        ?string $password = null,
        ?string $database = null,
        ?int $port = null,
        ?string $socket = null
    ): bool {
        // Alias of the constructor's connect path (php-src keeps it for
        // backwards compatibility with the old procedural object).
        return $this->real_connect($hostname, $username, $password, $database, $port, $socket, 0);
    }

    public function close(): bool {
        if ($this->conn >= 0) {
            // Finalize any live multi_query batch statement before the
            // connection handle goes back to the bridge.
            $this->multiClose();
            // Roll an open transaction back first (matching PHP and keeping a
            // persistent handle clean when it returns to the pool).
            if (elephc_pdo_in_transaction($this->conn) === 1) {
                elephc_pdo_rollback($this->conn);
            }
            elephc_pdo_release($this->conn, 0);
            $this->conn = -1;
            // An unconsumed buffered result dies with the connection (php-src
            // frees all pending state on close): without this, a later
            // real_connect() on the same object — which skips its own cleanup
            // because $conn is already -1 — would inherit the stale pending
            // result and raise a spurious 2014 on the first statement.
            $this->pendingResult = null;
        }
        return true;
    }

    public function __destruct() {
        $this->close();
    }

    public function ping(): bool {
        if (!$this->requireConnection()) {
            return false;
        }
        if (!$this->requireNoPendingResults()) {
            return false;
        }
        // A cheap round-trip; see the plan's elephc_pdo_ping escape hatch if
        // this ever eats a pending multi_query result.
        if (elephc_pdo_exec($this->conn, "SELECT 1") < 0) {
            return $this->opFailed();
        }
        $this->clearError();
        return true;
    }

    public function select_db(string $database): bool {
        if (!$this->requireConnection()) {
            return false;
        }
        if (!$this->requireNoPendingResults()) {
            return false;
        }
        $_ident = str_replace("`", "``", $database);
        if (elephc_pdo_exec($this->conn, "USE `" . $_ident . "`") < 0) {
            return $this->opFailed();
        }
        $this->clearError();
        return true;
    }

    public function set_charset(string $charset): bool {
        if (!$this->requireConnection()) {
            return false;
        }
        if (!$this->requireNoPendingResults()) {
            return false;
        }
        // Same [A-Za-z0-9_] identifier filter the PDO DSN charset key uses: a
        // charset name is an identifier, never quoted, so anything else is
        // rejected before it can smuggle SQL.
        if (!$this->charsetIdentIsValid($charset)) {
            $this->syntheticFailure(2019, "Invalid characterset or character set not supported", "HY000");
            return false;
        }
        if (elephc_pdo_exec($this->conn, "SET NAMES " . $charset) < 0) {
            return $this->opFailed();
        }
        $this->clearError();
        return true;
    }

    public function character_set_name(): string {
        // php 8: an Error on an unconnected object (not "").
        $this->requireInitialized("mysqli::character_set_name");
        // The connection's live charset, tracked by the bridge from the
        // handshake + every SET NAMES (no round-trip); true even after a
        // MYSQLI_INIT_COMMAND "SET NAMES …" or a reused persistent connection.
        return elephc_pdo_mysql_charset($this->conn);
    }

    public function real_escape_string(string $string): string {
        // php 8: real_escape_string on an unconnected object is an Error.
        $this->requireInitialized("mysqli::real_escape_string");
        // SECURITY: charset-aware escaping through the bridge, which uses the
        // connection's OWN tracked charset (never assuming utf8mb4) and its
        // NO_BACKSLASH_ESCAPES state — pure byte substitution is injectable
        // under gbk/big5/sjis/cp932/euckr/… (the trailing-byte breakout). The
        // length-counted read preserves embedded NUL bytes.
        $_len = elephc_pdo_real_escape_string($this->conn, $string, strlen($string));
        if ($_len < 0) {
            return "";
        }
        if ($_len == 0) {
            return "";
        }
        return __elephc_ptr_read_string(elephc_pdo_blob_data_ptr(), $_len);
    }

    public function quote_string(string $string): string {
        $this->requireInitialized("mysqli::quote_string");
        return "'" . $this->real_escape_string($string) . "'";
    }

    public function escape_string(string $string): string {
        return $this->real_escape_string($string);
    }

    public function begin_transaction(int $flags = 0, ?string $name = null): bool {
        // php-src: $name is a SQL COMMENT on START TRANSACTION, never a
        // savepoint; the empty-name ValueError is raised BEFORE any SQL goes to
        // the server (unlike the old ordering, which left an open transaction).
        $_comment = $this->transactionComment("mysqli::begin_transaction", $name, true);
        if (!$this->requireConnection()) {
            return false;
        }
        if (!$this->requireNoPendingResults()) {
            return false;
        }
        // php composes the whole statement once (captured wire:
        // "START TRANSACTION WITH CONSISTENT SNAPSHOT, READ WRITE"); the bridge's
        // note_transaction_sql updates in_transaction from raw START TRANSACTION
        // too, so close()/__destruct still auto-rollback.
        $_parts = [];
        if (($flags & 1) != 0) {
            $_parts[] = "WITH CONSISTENT SNAPSHOT";
        }
        if (($flags & 2) != 0) {
            $_parts[] = "READ WRITE";
        } elseif (($flags & 4) != 0) {
            $_parts[] = "READ ONLY";
        }
        $_sql = "START TRANSACTION";
        if (count($_parts) > 0) {
            $_sql = $_sql . " " . implode(", ", $_parts);
        }
        $_sql = $_sql . $_comment;
        if (elephc_pdo_exec($this->conn, $_sql) < 0) {
            return $this->opFailed();
        }
        $this->refreshStatus();
        $this->clearError();
        return true;
    }

    public function commit(int $flags = 0, ?string $name = null): bool {
        $_comment = $this->transactionComment("mysqli::commit", $name, false);
        if (!$this->requireConnection()) {
            return false;
        }
        if (!$this->requireNoPendingResults()) {
            return false;
        }
        // php: COMMIT [AND [NO] CHAIN] [[NO] RELEASE] /*name*/ — a real COMMIT,
        // never a RELEASE SAVEPOINT (the old code never actually committed).
        $_sql = "COMMIT" . $this->transactionCorFlags($flags) . $_comment;
        if (elephc_pdo_exec($this->conn, $_sql) < 0) {
            return $this->opFailed();
        }
        $this->refreshStatus();
        $this->clearError();
        return true;
    }

    public function rollback(int $flags = 0, ?string $name = null): bool {
        $_comment = $this->transactionComment("mysqli::rollback", $name, false);
        if (!$this->requireConnection()) {
            return false;
        }
        if (!$this->requireNoPendingResults()) {
            return false;
        }
        $_sql = "ROLLBACK" . $this->transactionCorFlags($flags) . $_comment;
        if (elephc_pdo_exec($this->conn, $_sql) < 0) {
            return $this->opFailed();
        }
        $this->refreshStatus();
        $this->clearError();
        return true;
    }

    public function savepoint(string $name): bool {
        if (!$this->requireConnection()) {
            return false;
        }
        if (!$this->requireNoPendingResults()) {
            return false;
        }
        if ($name === "") {
            throw new ValueError("mysqli::savepoint(): Argument #1 (\$name) cannot be empty");
        }
        if (elephc_pdo_exec($this->conn, "SAVEPOINT `" . str_replace("`", "``", $name) . "`") < 0) {
            return $this->opFailed();
        }
        $this->refreshStatus();
        $this->clearError();
        return true;
    }

    public function release_savepoint(string $name): bool {
        if (!$this->requireConnection()) {
            return false;
        }
        if (!$this->requireNoPendingResults()) {
            return false;
        }
        if ($name === "") {
            throw new ValueError("mysqli::release_savepoint(): Argument #1 (\$name) cannot be empty");
        }
        if (elephc_pdo_exec($this->conn, "RELEASE SAVEPOINT `" . str_replace("`", "``", $name) . "`") < 0) {
            return $this->opFailed();
        }
        $this->refreshStatus();
        $this->clearError();
        return true;
    }

    public function autocommit(bool $enable): bool {
        if (!$this->requireConnection()) {
            return false;
        }
        if (!$this->requireNoPendingResults()) {
            return false;
        }
        if (elephc_pdo_set_autocommit($this->conn, $enable ? 1 : 0) != 1) {
            return $this->opFailed();
        }
        $this->clearError();
        return true;
    }

    public function options(int $option, mixed $value): bool {
        // The three locked option ids; anything else is unsupported and fails
        // loudly with `false` (php-src also returns false for unknown options).
        if ($option == 0) {
            // MYSQLI_OPT_CONNECT_TIMEOUT -> DSN connect_timeout at connect.
            $this->optConnectTimeout = (int) $value;
            return true;
        }
        if ($option == 3) {
            // MYSQLI_INIT_COMMAND -> $my_init_command at connect.
            $this->optInitCommand = (string) $value;
            return true;
        }
        if ($option == 7) {
            // MYSQLI_SET_CHARSET_NAME -> SET NAMES after connect.
            $this->optCharsetName = (string) $value;
            return true;
        }
        return false;
    }

    public function set_opt(int $option, mixed $value): bool {
        return $this->options($option, $value);
    }

    public function query(string $query, int $resultmode = 0): mysqli_result|bool {
        if ($query === "") {
            throw new ValueError("mysqli::query(): Argument #1 (\$query) must not be empty");
        }
        if ($resultmode != 0 && $resultmode != 1) {
            throw new ValueError("mysqli::query(): Argument #2 (\$result_mode) must be either MYSQLI_STORE_RESULT or MYSQLI_USE_RESULT");
        }
        // MYSQLI_USE_RESULT (1) is accepted but still buffered — documented
        // divergence; true unbuffered use_result is out of scope.
        $_code = $this->runQuery($query);
        if ($_code == 0) {
            return false;
        }
        if ($_code == 1) {
            return true;
        }
        $_result = $this->pendingResult;
        $this->pendingResult = null;
        if ($_result === null) {
            return false;
        }
        return $_result;
    }

    public function prepare(string $query): mysqli_stmt|false {
        if ($query === "") {
            throw new ValueError("mysqli::prepare(): Argument #1 (\$query) must not be empty");
        }
        if (!$this->requireConnection()) {
            return false;
        }
        if (!$this->requireNoPendingResults()) {
            return false;
        }
        // Native (non-emulated) prepare: real `?` placeholders on the server.
        $_handle = elephc_pdo_prepare($this->conn, $query, 0);
        if ($_handle < 0) {
            $this->opFailed();
            return false;
        }
        $this->clearError();
        return mysqli_stmt::__elephcFromPrepare($this, $this->conn, $_handle, $query);
    }

    public function stmt_init(): mysqli_stmt {
        // The two-step form: an unprepared statement to be prepared with
        // $stmt->prepare($sql) (or mysqli_stmt_prepare()).
        $this->requireInitialized("mysqli::stmt_init");
        return mysqli_stmt::__elephcInit($this, $this->conn);
    }

    public function get_charset(): mixed {
        // php returns a stdClass describing the connection charset. elephc knows
        // the negotiated name (utf8mb4 by default / whatever set_charset set);
        // the numeric collation/state/comment fields the bridge does not expose
        // are reported as 0 / "" (documented). The common `->charset` read is
        // exact.
        $this->requireInitialized("mysqli::get_charset");
        $_info = new stdClass();
        $_info->{"charset"} = elephc_pdo_mysql_charset($this->conn);
        $_info->{"collation"} = "";
        $_info->{"dir"} = "";
        $_info->{"min_length"} = 0;
        $_info->{"max_length"} = 0;
        $_info->{"number"} = 0;
        $_info->{"state"} = 0;
        $_info->{"comment"} = "";
        return $_info;
    }

    // -- elephc PHP >= 8.2 mysqli execute_query begin --
    public function execute_query(string $query, ?array $params = null): mysqli_result|bool {
        // prepare + execute($params) + get_result in one call (PHP 8.2+).
        $_statement = $this->prepare($query);
        if ($_statement === false) {
            return false;
        }
        if (!$_statement->execute($params)) {
            $_statement->close();
            return false;
        }
        if ($_statement->field_count == 0) {
            // Non-select: mirror the statement's outcome on the connection and
            // report success as `true`, like mysqli::query does.
            $this->affected_rows = $_statement->affected_rows;
            $this->insert_id = $_statement->insert_id;
            $this->field_count = 0;
            $_statement->close();
            return true;
        }
        $_result = $_statement->get_result();
        $_statement->close();
        if ($_result === false) {
            return false;
        }
        $this->field_count = $_result->field_count;
        return $_result;
    }
    // -- elephc PHP >= 8.2 mysqli execute_query end --

    public function real_query(string $query): bool {
        if ($query === "") {
            throw new ValueError("mysqli::real_query(): Argument #1 (\$query) must not be empty");
        }
        // Same drain as query(); a produced result set stays pending until
        // store_result() picks it up.
        return $this->runQuery($query) != 0;
    }

    public function multi_query(string $query): bool {
        if ($query === "") {
            throw new ValueError("mysqli::multi_query(): Argument #1 (\$query) must not be empty");
        }
        if (!$this->requireConnection()) {
            return false;
        }
        if (!$this->requireNoPendingResults()) {
            return false;
        }
        // One server round-trip for the whole batch: the bridge's emulated
        // prepare + step executes the string with multi-statements enabled
        // (mysqlnd's own default, mirrored by the bridge) and retains every
        // wire result set for elephc_pdo_next_rowset.
        $this->multiClose();
        $_stmt = elephc_pdo_prepare($this->conn, $query, 1);
        if ($_stmt < 0) {
            return $this->opFailed();
        }
        $_rc = elephc_pdo_step($_stmt);
        if ($_rc < 0) {
            $this->opFailed();
            elephc_pdo_finalize($_stmt);
            return false;
        }
        $this->multiStmt = $_stmt;
        return $this->multiDrainCurrent($_rc);
    }

    public function more_results(): bool {
        return $this->multiMore;
    }

    public function next_result(): bool {
        // The probe in multiDrainCurrent already advanced the statement onto
        // the next retained result set; step its first row and drain it.
        if (!$this->multiMore || $this->multiStmt < 0) {
            $this->multiMore = false;
            return false;
        }
        $_rc = elephc_pdo_step($this->multiStmt);
        if ($_rc < 0) {
            $this->opFailed();
            $this->multiClose();
            return false;
        }
        return $this->multiDrainCurrent($_rc);
    }

    public function store_result(int $mode = 0): mysqli_result|false {
        $_result = $this->pendingResult;
        if ($_result === null) {
            return false;
        }
        $this->pendingResult = null;
        return $_result;
    }

    public function use_result(): mysqli_result|false {
        // Alias of store_result: results are always buffered (documented
        // divergence; true unbuffered streaming is out of scope).
        return $this->store_result();
    }

    public function get_server_info(): string {
        return $this->server_info;
    }

    public function get_client_info(): string {
        return $this->client_info;
    }

    public function get_host_info(): string {
        return $this->host_info;
    }

    public function get_proto_info(): int {
        return $this->protocol_version;
    }

    public function get_server_version(): int {
        return $this->server_version;
    }

    public function get_client_version(): int {
        return $this->client_version;
    }

    public function stat(): string|false {
        if (!$this->requireConnection()) {
            return false;
        }
        if (!$this->requireNoPendingResults()) {
            return false;
        }
        // The bridge's server_info is MySQL's own "Uptime: … Questions: …"
        // statistics line — exactly what mysqli::stat() returns.
        $_stat = elephc_pdo_server_info($this->conn);
        if ($_stat === "") {
            return false;
        }
        return $_stat;
    }

    // -- internal helpers ($_-prefixed locals; same checker rule as PDO) --

    // Guards statement-issuing operations while multi_query result sets (or a
    // real_query result) remain unconsumed: php-src raises
    // CR_COMMANDS_OUT_OF_SYNC (2014) there, and silently mixing the pending
    // batch state with a new statement would corrupt store_result/next_result.
    private function requireNoPendingResults(): bool {
        if ($this->__elephcHasPendingResults()) {
            $this->syntheticFailure(2014, "Commands out of sync; you can't run this command now", "HY000");
            return false;
        }
        return true;
    }

    // Read-only pending probe for mysqli_stmt's two-step prepare()/execute()
    // guard: the stmt records the 2014 on ITS OWN error state (php-src puts a
    // busy-connection failure on the statement), so it needs the condition,
    // not requireNoPendingResults' connection-level failure. Private — the
    // checker's mysqli friend channel exposes it to mysqli_stmt only, keeping
    // it off PHP's mysqli surface.
    private function __elephcHasPendingResults(): bool {
        return $this->multiStmt >= 0 || $this->multiMore || $this->pendingResult !== null;
    }

    // Renders a transaction $name as php-src's ` /*name*/` SQL comment suffix
    // ("" when $name is null). An empty $name is a ValueError ONLY for
    // begin_transaction ($emptyThrows) — php throws there but for commit /
    // rollback it does not, sending `COMMIT /**/` / `ROLLBACK /**/` instead.
    // A non-empty name is STRIPPED to php's allowlist [A-Za-z0-9 -_=] before it
    // is wrapped: blocklisting `*/` was not enough because a name starting with
    // `!` (or MariaDB's `M!`) opens an EXECUTABLE `/*! … */` comment whose body
    // the server runs — a `;` inside it would then execute a second statement,
    // and this path (exec, not runQuery) never sees the multi-statement guard.
    // Stripping to the allowlist closes every comment dialect at once, exactly
    // as php sanitises the name.
    private function transactionComment(string $context, ?string $name, bool $emptyThrows): string {
        if ($name === null) {
            return "";
        }
        if ($name === "") {
            if ($emptyThrows) {
                throw new ValueError($context . "(): Argument #2 (\$name) must not be empty");
            }
            return " /**/";
        }
        $_clean = "";
        $_len = strlen($name);
        for ($_i = 0; $_i < $_len; $_i++) {
            $_c = substr($name, $_i, 1);
            $_o = ord($_c);
            // php's kept set (empirically, feeding every printable byte):
            // space, `-`, 0-9, `=`, A-Z, `_`, a-z. Backslash is NOT kept, though
            // php's own warning text lists it; the empirical behaviour wins.
            if (($_o >= 48 && $_o <= 57)          // 0-9
                || ($_o >= 65 && $_o <= 90)       // A-Z
                || ($_o >= 97 && $_o <= 122)      // a-z
                || $_c === " " || $_c === "-" || $_c === "_" || $_c === "=") {
                $_clean = $_clean . $_c;
            }
        }
        // php raises an E_WARNING ("Transaction name has been truncated …") when
        // it drops characters; elephc has no E_WARNING channel, so the stripping
        // is silent (documented divergence — the security behaviour is identical).
        return " /*" . $_clean . "*/";
    }

    // Renders the COMMIT/ROLLBACK completion flags (MYSQLI_TRANS_COR_*), matching
    // php-src's mysqlnd: AND [NO] CHAIN and [NO] RELEASE, each emitted only when
    // its bit is set and its opposite is not.
    private function transactionCorFlags(int $flags): string {
        $_sql = "";
        if (($flags & 1) != 0 && ($flags & 2) == 0) {
            $_sql = $_sql . " AND CHAIN";
        } elseif (($flags & 2) != 0 && ($flags & 1) == 0) {
            $_sql = $_sql . " AND NO CHAIN";
        }
        if (($flags & 4) != 0 && ($flags & 8) == 0) {
            $_sql = $_sql . " RELEASE";
        } elseif (($flags & 8) != 0 && ($flags & 4) == 0) {
            $_sql = $_sql . " NO RELEASE";
        }
        return $_sql;
    }

    // Refreshes the connection status fields from the last command's OK packet,
    // matching php (which updates them after EVERY command). A control statement
    // like COMMIT / BEGIN / SET NAMES reports affected_rows = 0, so this resets
    // the DML count a prior query() left behind — e.g. `$db->affected_rows`
    // read after commit() is 0, as in php, not the previous statement's count.
    private function refreshStatus(): void {
        $this->affected_rows = elephc_pdo_changes($this->conn);
        $this->insert_id = elephc_pdo_last_insert_id($this->conn, "");
        $this->warning_count = elephc_pdo_warning_count($this->conn);
    }

    // php 8 raises `Error: mysqli object is not fully initialized` for any
    // operation on a `mysqli_init()` / argument-less `new mysqli()` object (or a
    // failed/closed connection), regardless of mysqli_report mode — it is an
    // Error, not a connection warning. Throwing here (rather than the old
    // silent 2006 `false`) closes the divergence where an unconnected
    // real_escape_string returned a value with no signal at all.
    private function requireInitialized(string $context): void {
        if ($this->conn < 0) {
            throw new Error($context . "(): mysqli object is not fully initialized");
        }
    }

    // Guards every operation that needs a live connection. An unconnected object
    // raises the same php Error as requireInitialized; always returns true when
    // it does not throw so existing `if (!requireConnection())` call sites stay
    // correct.
    private function requireConnection(): bool {
        $this->requireInitialized("mysqli");
        return true;
    }

    // Records a client-side failure that has no live bridge error state
    // (unconnected handle, invalid charset) and dispatches mysqli_report.
    private function syntheticFailure(int $errno, string $message, string $sqlstate): void {
        $this->errno = $errno;
        $this->error = $message;
        $this->sqlstate = $sqlstate;
        $this->error_list = [["errno" => $errno, "sqlstate" => $sqlstate, "error" => $message]];
        $this->report($message, $errno, $sqlstate);
    }

    // Records a connect-time failure on both the instance (connect_errno /
    // connect_error, distinct from errno/error) and the process-wide statics
    // behind mysqli_connect_errno()/mysqli_connect_error(), then dispatches
    // mysqli_report. Always returns false so connect paths can tail-call it.
    private function connectFailure(int $errno, string $message, string $sqlstate): bool {
        $this->connect_errno = $errno;
        $this->connect_error = $message;
        $this->errno = $errno;
        $this->error = $message;
        $this->sqlstate = $sqlstate;
        $this->error_list = [["errno" => $errno, "sqlstate" => $sqlstate, "error" => $message]];
        mysqli::$lastConnectErrno = $errno;
        mysqli::$lastConnectError = $message;
        $this->report($message, $errno, $sqlstate);
        return false;
    }

    // Refreshes errno/error/sqlstate/error_list from the live bridge error
    // state after a failed operation, then dispatches mysqli_report. Always
    // returns false so callers can tail-call it.
    private function opFailed(): bool {
        $this->errno = elephc_pdo_errcode($this->conn);
        $this->error = elephc_pdo_errmsg($this->conn);
        $this->sqlstate = elephc_pdo_sqlstate($this->conn);
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

    // mysqli_report dispatch: STRICT throws mysqli_sql_exception (never
    // PDOException), ERROR alone writes the message to STDERR, OFF is silent.
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

    // Executes one statement through the bridge and fully buffers any result
    // set (result identity: a later query must never invalidate an earlier
    // mysqli_result, so every row is drained and the statement finalized before
    // this returns). Returns 0 = failure (error state recorded and reported),
    // 1 = success with no result set (DML/DDL: affected_rows/insert_id set),
    // 2 = success with a result buffered into $this->pendingResult.
    private function runQuery(string $query): int {
        if (!$this->requireConnection()) {
            return 0;
        }
        if (!$this->requireNoPendingResults()) {
            return 0;
        }
        // php-src rejects multi-statement strings in mysqli_query (mysqlnd
        // toggles CLIENT_MULTI_STATEMENTS per multi_query call via
        // COM_SET_OPTION; the bridge keeps it enabled for the whole
        // connection), so a classic "1; DROP TABLE …" injection would
        // otherwise EXECUTE here. The rejection uses the ONE authoritative
        // bridge scanner (which treats `/*! … */` executable comments as live
        // SQL, closing the comment-hidden separator bypass); a fast strpos
        // skips the scan for the overwhelming majority of statements that carry
        // no ';' at all. multi_query() is the one multi-statement path.
        if (strpos($query, ";") !== false
            && elephc_pdo_sql_has_multiple_statements($this->conn, $query, strlen($query)) != 0) {
            $this->syntheticFailure(1064, "elephc mysqli does not support multiple statements in mysqli::query(); use mysqli::multi_query()", "42000");
            return 0;
        }
        $_stmt = elephc_pdo_prepare($this->conn, $query, 1);
        if ($_stmt < 0) {
            $this->opFailed();
            return 0;
        }
        $_rc = elephc_pdo_step($_stmt);
        if ($_rc < 0) {
            $this->opFailed();
            elephc_pdo_finalize($_stmt);
            return 0;
        }
        // Column metadata is definitely known after the first step, including
        // for emulated prepares that only execute at step time.
        $_columnCount = elephc_pdo_column_count($_stmt);
        if ($_columnCount == 0) {
            $this->affected_rows = elephc_pdo_changes($this->conn);
            $this->insert_id = elephc_pdo_last_insert_id($this->conn, "");
            $this->field_count = 0;
            $this->warning_count = elephc_pdo_warning_count($this->conn);
            elephc_pdo_finalize($_stmt);
            $this->clearError();
            $this->pendingResult = null;
            return 1;
        }
        $_names = [];
        $_tables = [];
        $_natives = [];
        $_flags = [];
        $_lens = [];
        $_decimals = [];
        for ($_i = 0; $_i < $_columnCount; $_i++) {
            $_names[] = elephc_pdo_column_name($_stmt, $_i);
            $_tables[] = elephc_pdo_column_table_name($_stmt, $_i);
            $_natives[] = elephc_pdo_column_native_type($_stmt, $_i);
            $_flags[] = elephc_pdo_column_flags($_stmt, $_i);
            $_lens[] = elephc_pdo_column_len($_stmt, $_i);
            $_decimals[] = elephc_pdo_column_precision($_stmt, $_i);
        }
        // Cells are buffered FLAT in row-major order (see mysqli_result): every
        // buffered value stays a Mixed scalar and fetches build fresh rows.
        $_cells = [];
        $_rowCount = 0;
        while ($_rc == 1) {
            for ($_i = 0; $_i < $_columnCount; $_i++) {
                $_cells[] = $this->columnValue($_stmt, $_i);
            }
            $_rowCount = $_rowCount + 1;
            $_rc = elephc_pdo_step($_stmt);
        }
        if ($_rc < 0) {
            $this->opFailed();
            elephc_pdo_finalize($_stmt);
            return 0;
        }
        elephc_pdo_finalize($_stmt);
        // php-src: for a SELECT, affected_rows mirrors num_rows and insert_id
        // resets to 0 (the statement generated no AUTO_INCREMENT value).
        $this->affected_rows = $_rowCount;
        $this->insert_id = 0;
        $this->field_count = $_columnCount;
        $this->warning_count = elephc_pdo_warning_count($this->conn);
        $this->clearError();
        $this->pendingResult = mysqli_result::__elephcFromDrain($_cells, $_rowCount, $_names, $_tables, $_natives, $_flags, $_lens, $_decimals);
        return 2;
    }

    // Drains the CURRENT result set of the active multi_query batch into
    // $this->pendingResult (or records affected_rows/insert_id for a
    // non-select set), then eagerly probes elephc_pdo_next_rowset so
    // more_results() can answer without consuming; the batch statement is
    // finalized as soon as the probe reports no further set. Returns false —
    // error recorded, batch closed, nothing buffered — if a step fails
    // mid-drain (same contract as runQuery's fetch loop; with the bridge's
    // buffered execution the first step surfaces server errors, so this is
    // the defensive twin for a mid-stream failure, e.g. a dropped
    // connection).
    private function multiDrainCurrent(int $firstStep): bool {
        $_stmt = $this->multiStmt;
        $_cols = elephc_pdo_column_count($_stmt);
        if ($_cols == 0) {
            $this->affected_rows = elephc_pdo_changes($this->conn);
            $this->insert_id = elephc_pdo_last_insert_id($this->conn, "");
            $this->field_count = 0;
            $this->pendingResult = null;
        } else {
            $_names = [];
            $_tables = [];
            $_natives = [];
            $_flags = [];
            $_lens = [];
            $_decimals = [];
            for ($_i = 0; $_i < $_cols; $_i++) {
                $_names[] = elephc_pdo_column_name($_stmt, $_i);
                $_tables[] = elephc_pdo_column_table_name($_stmt, $_i);
                $_natives[] = elephc_pdo_column_native_type($_stmt, $_i);
                $_flags[] = elephc_pdo_column_flags($_stmt, $_i);
                $_lens[] = elephc_pdo_column_len($_stmt, $_i);
                $_decimals[] = elephc_pdo_column_precision($_stmt, $_i);
            }
            $_cells = [];
            $_rowCount = 0;
            $_rc = $firstStep;
            while ($_rc == 1) {
                for ($_i = 0; $_i < $_cols; $_i++) {
                    $_cells[] = $this->columnValue($_stmt, $_i);
                }
                $_rowCount = $_rowCount + 1;
                $_rc = elephc_pdo_step($_stmt);
            }
            if ($_rc < 0) {
                // A step failed mid-set: report the error instead of passing
                // off the truncated buffer as a complete result, and close
                // the batch — its remaining sets are not trustworthy.
                $this->opFailed();
                $this->multiClose();
                $this->pendingResult = null;
                return false;
            }
            $this->affected_rows = $_rowCount;
            $this->insert_id = 0;
            $this->field_count = $_cols;
            $this->pendingResult = mysqli_result::__elephcFromDrain($_cells, $_rowCount, $_names, $_tables, $_natives, $_flags, $_lens, $_decimals);
        }
        $this->warning_count = elephc_pdo_warning_count($this->conn);
        $this->clearError();
        $this->multiMore = elephc_pdo_next_rowset($_stmt) == 1;
        if (!$this->multiMore) {
            elephc_pdo_finalize($_stmt);
            $this->multiStmt = -1;
        }
        return true;
    }

    // Finalizes any active multi_query batch statement and clears its state.
    private function multiClose(): void {
        if ($this->multiStmt >= 0) {
            elephc_pdo_finalize($this->multiStmt);
            $this->multiStmt = -1;
        }
        $this->multiMore = false;
    }

    // Decodes one cell of the current row, same type dispatch as
    // PDOStatement::fetch's columnValue (int / float / null / length-counted
    // TEXT-or-BLOB copy so embedded NUL bytes survive); mysqli has no
    // stringify/oracle-nulls modes, so those branches are absent.
    private function columnValue(int $stmt, int $index): mixed {
        $_type = elephc_pdo_column_type($stmt, $index);
        if ($_type == 1) {
            return elephc_pdo_column_int($stmt, $index);
        }
        if ($_type == 2) {
            return elephc_pdo_column_double($stmt, $index);
        }
        if ($_type == 5) {
            return null;
        }
        // The $_len == 0 guard is load-bearing: the bridge returns a NULL
        // pointer for an empty buffer and ptr_read_string fatals on NULL.
        $_len = elephc_pdo_column_data_len($stmt, $index);
        if ($_len > 0) {
            return __elephc_ptr_read_string(elephc_pdo_column_data_ptr($stmt, $index), $_len);
        }
        return "";
    }

    // "8.0.36-log" -> 80036, php-src's major*10000 + minor*100 + patch. The
    // bridge's client string is "mysql x.y.z"; strip that prefix first.
    private function versionStringToInt(string $version): int {
        if (str_starts_with($version, "mysql ")) {
            $version = substr($version, 6);
        }
        $_parts = explode(".", $version);
        $_major = (int) $_parts[0];
        $_minor = count($_parts) > 1 ? (int) $_parts[1] : 0;
        $_patch = count($_parts) > 2 ? (int) $_parts[2] : 0;
        return $_major * 10000 + $_minor * 100 + $_patch;
    }

    // [A-Za-z0-9_] identifier filter shared by set_charset, same rule the PDO
    // DSN charset key enforces bridge-side.
    private function charsetIdentIsValid(string $charset): bool {
        $_len = strlen($charset);
        if ($_len == 0) {
            return false;
        }
        for ($_i = 0; $_i < $_len; $_i++) {
            $_c = ord(substr($charset, $_i, 1));
            $_ok = ($_c >= 48 && $_c <= 57) || ($_c >= 65 && $_c <= 90) || ($_c >= 97 && $_c <= 122) || $_c == 95;
            if (!$_ok) {
                return false;
            }
        }
        return true;
    }
}
"##;
